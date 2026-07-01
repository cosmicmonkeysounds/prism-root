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
 * `s.lines()` — split on `\n`, dropping the line terminator and a
 * trailing `\r` (so `\r\n` collapses too). An empty string yields no
 * lines, and a trailing `\n` does not produce a final empty line —
 * matching Rust's `str::lines`.
 */
export function lines(source: string): string[] {
  const out: string[] = [];
  let start = 0;
  while (start <= source.length) {
    const nl = source.indexOf("\n", start);
    if (nl < 0) {
      if (start < source.length) out.push(stripCarriageReturn(source.slice(start)));
      break;
    }
    out.push(stripCarriageReturn(source.slice(start, nl)));
    start = nl + 1;
  }
  return out;
}

function stripCarriageReturn(s: string): string {
  return s.endsWith("\r") ? s.slice(0, -1) : s;
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

/**
 * Split on commas at bracket depth 0, so `Scanner(a, b), Algo` splits into
 * `["Scanner(a, b)", " Algo"]` — a comma inside `()` / `[]` / `{}` is an
 * argument separator, not a top-level split. Callers trim the pieces.
 */
export function splitTopLevelCommas(text: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i]!;
    if (ch === "(" || ch === "[" || ch === "{") depth += 1;
    else if (ch === ")" || ch === "]" || ch === "}") depth -= 1;
    else if (ch === "," && depth === 0) {
      out.push(text.slice(start, i));
      start = i + 1;
    }
  }
  out.push(text.slice(start));
  return out;
}
