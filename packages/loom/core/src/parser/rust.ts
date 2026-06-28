//! Small helpers that mirror the Rust `str` idioms the parser leans on,
//! so the TypeScript port can stay line-for-line close to the original.
//!
//! Offsets throughout the port are JS string offsets (UTF-16 code
//! units), not UTF-8 bytes. For ASCII source — which every test input
//! and the overwhelming majority of `.loom` content is — these are
//! identical to the Rust byte offsets. For non-ASCII content they are
//! internally consistent and, conveniently, exactly what CodeMirror /
//! the LSP UTF-16 boundary want.

/** `s.strip_prefix(p)` — the remainder after `p`, or `null` if absent. */
export function stripPrefix(s: string, p: string): string | null {
  return s.startsWith(p) ? s.slice(p.length) : null;
}

/** `s.strip_suffix(p)` — the head before `p`, or `null` if absent. */
export function stripSuffix(s: string, p: string): string | null {
  return s.endsWith(p) ? s.slice(0, s.length - p.length) : null;
}

/** `s.split_once(sep)` — `[before, after]` at the first `sep`, or `null`. */
export function splitOnce(s: string, sep: string): [string, string] | null {
  const idx = s.indexOf(sep);
  if (idx < 0) return null;
  return [s.slice(0, idx), s.slice(idx + sep.length)];
}

/** `s.rsplit_once(sep)` — `[before, after]` at the last `sep`, or `null`. */
export function rsplitOnce(s: string, sep: string): [string, string] | null {
  const idx = s.lastIndexOf(sep);
  if (idx < 0) return null;
  return [s.slice(0, idx), s.slice(idx + sep.length)];
}

/** `s.trim_start_matches(p)` — strip every leading occurrence of `p`. */
export function trimStartMatches(s: string, p: string): string {
  if (p.length === 0) return s;
  let out = s;
  while (out.startsWith(p)) out = out.slice(p.length);
  return out;
}

/** `s.trim_end_matches(p)` — strip every trailing occurrence of `p`. */
export function trimEndMatches(s: string, p: string): string {
  if (p.length === 0) return s;
  let out = s;
  while (out.endsWith(p)) out = out.slice(0, out.length - p.length);
  return out;
}

/**
 * `s.parse::<f64>()` — strict float parse. Rust requires the whole
 * (already-trimmed) string to be a valid float; trailing garbage fails.
 */
export function parseF64(s: string): number | null {
  if (s.length === 0) return null;
  // Number("") === 0 and Number(" ") === 0 — guard whitespace-only.
  if (s.trim().length !== s.length) return null;
  const n = Number(s);
  return Number.isNaN(n) ? null : n;
}

/** `s.parse::<u32>()` — non-negative integer, optional leading `+`. */
export function parseU32(s: string): number | null {
  if (!/^\+?\d+$/.test(s)) return null;
  const n = Number.parseInt(s, 10);
  return Number.isSafeInteger(n) ? n : null;
}

/** ASCII alphanumeric — Rust `char::is_ascii_alphanumeric`. */
export function isAsciiAlphanumeric(ch: string): boolean {
  return /^[0-9A-Za-z]$/.test(ch);
}

/** ASCII alphabetic — Rust `char::is_ascii_alphabetic`. */
export function isAsciiAlphabetic(ch: string): boolean {
  return /^[A-Za-z]$/.test(ch);
}

/** ASCII uppercase — Rust `char::is_ascii_uppercase`. */
export function isAsciiUppercase(ch: string): boolean {
  return /^[A-Z]$/.test(ch);
}

/** True if every char of `s` satisfies `pred`. (`s.chars().all(pred)`) */
export function allChars(s: string, pred: (ch: string) => boolean): boolean {
  for (const ch of s) {
    if (!pred(ch)) return false;
  }
  return true;
}

/** True if any char of `s` satisfies `pred`. (`s.chars().any(pred)`) */
export function anyChar(s: string, pred: (ch: string) => boolean): boolean {
  for (const ch of s) {
    if (pred(ch)) return true;
  }
  return false;
}

/**
 * `source.split_inclusive('\n')` — each chunk keeps its trailing `\n`.
 * A trailing chunk without a final newline is included as-is. An empty
 * source yields no chunks (matching Rust's `split_inclusive`).
 */
export function splitInclusive(source: string, sep: string): string[] {
  if (source.length === 0) return [];
  const out: string[] = [];
  let start = 0;
  let idx = source.indexOf(sep, start);
  while (idx >= 0) {
    out.push(source.slice(start, idx + sep.length));
    start = idx + sep.length;
    idx = source.indexOf(sep, start);
  }
  if (start < source.length) {
    out.push(source.slice(start));
  }
  return out;
}
