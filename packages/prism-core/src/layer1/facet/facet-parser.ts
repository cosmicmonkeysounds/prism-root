/**
 * Facet value parsing and serialization — YAML/JSON to/from key-value records.
 *
 * Uses Scanner for structured parsing — no regex.
 * Used by facet surfaces to parse source text into editable values
 * and serialize changes back to source format.
 */

import { Scanner, isDigit } from '../syntax/scanner.js';
import type { FieldSchema } from '../forms/index.js';

// ── Types ────────────────────────────────────────────────────────────────────

export type SourceFormat = 'yaml' | 'json';

// ── Format detection ─────────────────────────────────────────────────────────

export function detectFormat(value: string): SourceFormat {
  const s = new Scanner(value);
  s.skipWhitespaceAndNewlines();
  const ch = s.peek();
  if (ch === '{' || ch === '[') return 'json';
  return 'yaml';
}

// ── Parsing ──────────────────────────────────────────────────────────────────

export function parseValues(value: string, format: SourceFormat): Record<string, unknown> {
  if (!value.trim()) return {};

  if (format === 'json') {
    try {
      const parsed: unknown = JSON.parse(value);
      return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
        ? (parsed as Record<string, unknown>)
        : { _root: parsed };
    } catch {
      return {};
    }
  }

  return parseYamlKeyValues(value);
}

/** Flat YAML key-value parser using Scanner. */
function parseYamlKeyValues(source: string): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const s = new Scanner(source);

  while (!s.isAtEnd) {
    // Skip leading whitespace on the line
    s.skipWhitespace();

    // Empty line or comment — skip to next line
    if (s.peek() === '\n' || s.peek() === '\r') { s.advance(); continue; }
    if (s.peek() === '#') { skipToEol(s); continue; }

    // Scan key — everything up to ':'
    const key = s.scanWhile(ch => ch !== ':' && ch !== '\n' && ch !== '\r').trim();
    if (!key || s.peek() !== ':') { skipToEol(s); continue; }
    s.advance(); // consume ':'

    // Scan value — rest of the line
    s.skipWhitespace();
    const rawVal = s.scanWhile(ch => ch !== '\n' && ch !== '\r').trim();

    result[key] = coerceYamlValue(rawVal);

    // Consume newline if present
    if (!s.isAtEnd) s.advance();
  }

  return result;
}

/** Coerce a raw YAML value string to a typed JS value. */
function coerceYamlValue(raw: string): unknown {
  if (raw === '' || raw === 'null' || raw === '~') return null;
  if (raw === 'true') return true;
  if (raw === 'false') return false;

  // Number detection via Scanner
  if (looksLikeNumber(raw)) {
    const n = Number(raw);
    if (!isNaN(n)) return n;
  }

  // Inline JSON array
  if (raw.startsWith('[') && raw.endsWith(']')) {
    try { return JSON.parse(raw) as unknown; } catch { /* keep as string */ }
  }

  // Strip quotes
  if (raw.length >= 2) {
    const q = raw[0];
    if ((q === '"' || q === "'") && raw[raw.length - 1] === q) {
      return raw.slice(1, -1);
    }
  }

  return raw;
}

/** Check if a string looks like a number using character inspection. */
function looksLikeNumber(val: string): boolean {
  if (val.length === 0) return false;
  const s = new Scanner(val);
  if (s.peek() === '-') s.advance();
  if (s.isAtEnd || !isDigit(s.peek())) return false;
  s.scanWhile(isDigit);
  if (s.peek() === '.') {
    s.advance();
    if (!isDigit(s.peek())) return false;
    s.scanWhile(isDigit);
  }
  return s.isAtEnd;
}

/** Advance Scanner past current line, consuming the newline. */
function skipToEol(s: Scanner): void {
  s.scanWhile(ch => ch !== '\n' && ch !== '\r');
  if (!s.isAtEnd) s.advance();
}

// ── Serialization ────────────────────────────────────────────────────────────

function serializeYamlValue(key: string, val: unknown): string {
  if (val === null || val === undefined) return `${key}:`;
  if (typeof val === 'boolean') return `${key}: ${val}`;
  if (typeof val === 'number') return `${key}: ${val}`;
  if (Array.isArray(val)) return `${key}: ${JSON.stringify(val)}`;
  const str = String(val);
  if (str.includes(':') || str.includes('#') || str.includes('"') || str.includes("'") || str === '') {
    // Escape double quotes via scanner walk
    let escaped = '';
    const sc = new Scanner(str);
    while (!sc.isAtEnd) {
      const ch = sc.advance();
      escaped += ch === '"' ? '\\"' : ch;
    }
    return `${key}: "${escaped}"`;
  }
  return `${key}: ${str}`;
}

/**
 * Serialize values back to source format.
 * For YAML, preserves comments and key ordering from the original source.
 */
export function serializeValues(
  values: Record<string, unknown>,
  format: SourceFormat,
  originalSource: string,
): string {
  if (format === 'json') {
    return JSON.stringify(values, null, 2);
  }

  // Preserve YAML comments and ordering from original source where possible
  const lines: string[] = [];
  const emitted = new Set<string>();

  for (const line of originalSource.split('\n')) {
    const ls = new Scanner(line);
    ls.skipWhitespace();

    // Blank line or comment — preserve
    if (ls.isAtEnd || ls.peek() === '#' || ls.peek() === '\n') {
      lines.push(line);
      continue;
    }

    // Try to extract key before ':'
    const key = ls.scanWhile(ch => ch !== ':' && ch !== '\n').trim();
    if (!key || ls.peek() !== ':') {
      lines.push(line);
      continue;
    }

    if (key in values) {
      lines.push(serializeYamlValue(key, values[key]));
      emitted.add(key);
    }
  }

  // Append any new keys not in original
  for (const [key, val] of Object.entries(values)) {
    if (!emitted.has(key)) {
      lines.push(serializeYamlValue(key, val));
    }
  }

  return lines.join('\n');
}

// ── Field inference ──────────────────────────────────────────────────────────

/**
 * Infer field schemas from parsed values when no explicit schema is provided.
 * Auto-detects: boolean, number, url, email, date, textarea, tags, text.
 */
export function inferFields(values: Record<string, unknown>): FieldSchema[] {
  return Object.entries(values).map(([key, val]): FieldSchema => {
    const label = humanizeKey(key);

    if (typeof val === 'boolean') return { id: key, label, type: 'boolean' };
    if (typeof val === 'number') return { id: key, label, type: 'number' };
    if (Array.isArray(val)) return { id: key, label, type: 'tags' };
    if (typeof val === 'string') {
      if (looksLikeUrl(val)) return { id: key, label, type: 'url' };
      if (val.includes('@') && val.includes('.')) return { id: key, label, type: 'email' };
      if (looksLikeDate(val)) return { id: key, label, type: 'date' };
      if (val.length > 80) return { id: key, label, type: 'textarea' };
    }
    return { id: key, label, type: 'text' };
  });
}

/** Convert a camelCase/snake_case/kebab-case key to a human-readable label. */
function humanizeKey(key: string): string {
  const s = new Scanner(key);
  const parts: string[] = [];
  let current = '';

  while (!s.isAtEnd) {
    const ch = s.advance();
    if (ch === '_' || ch === '-') {
      if (current) { parts.push(current); current = ''; }
    } else if (ch >= 'A' && ch <= 'Z') {
      if (current) { parts.push(current); current = ''; }
      current += ch;
    } else {
      current += ch;
    }
  }
  if (current) parts.push(current);

  if (parts.length === 0) return key;
  const first = parts[0] ?? '';
  parts[0] = (first[0] ?? '').toUpperCase() + first.slice(1);
  return parts.join(' ');
}

/** Check if a value starts with http:// or https:// */
function looksLikeUrl(val: string): boolean {
  const s = new Scanner(val);
  return s.match('https://') || s.match('http://');
}

/** Check if a value matches YYYY-MM-DD pattern. */
function looksLikeDate(val: string): boolean {
  if (val.length < 10) return false;
  const s = new Scanner(val);
  // 4 digits
  for (let i = 0; i < 4; i++) { if (!isDigit(s.peek())) return false; s.advance(); }
  if (!s.match('-')) return false;
  // 2 digits
  for (let i = 0; i < 2; i++) { if (!isDigit(s.peek())) return false; s.advance(); }
  if (!s.match('-')) return false;
  // 2 digits
  for (let i = 0; i < 2; i++) { if (!isDigit(s.peek())) return false; s.advance(); }
  return true;
}
