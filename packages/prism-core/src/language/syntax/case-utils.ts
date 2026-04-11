/**
 * Case-conversion utilities for codegen.
 *
 * Shared by all emitters and Prism packages. Single source of truth for
 * toCamel/toPascal/toScreamingSnake/toSnakeCase conversions.
 */

// ── Internal: split into words ──────────────────────────────────────────────

/**
 * Split a string into words, handling:
 *   snake_case, kebab-case, dot.case, camelCase, PascalCase, SCREAMING_SNAKE
 */
function splitWords(s: string): string[] {
  return s
    .replace(/[^a-zA-Z0-9]/g, ' ')              // snake_case, kebab-case, dot.case → spaces
    .replace(/([a-z])([A-Z])/g, '$1 $2')          // camelCase boundary
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2')    // HTMLParser → HTML Parser
    .trim()
    .split(/\s+/)
    .filter(Boolean);
}

// ── Public API ──────────────────────────────────────────────────────────────

/** Strip non-identifier characters, ensure starts with letter or underscore. */
export function safeIdentifier(s: string): string {
  const clean = s.replace(/[^a-zA-Z0-9_]/g, '_').replace(/^([0-9])/, '_$1');
  return clean || '_unknown';
}

/** Convert any case to camelCase. */
export function toCamelCase(s: string): string {
  const words = splitWords(s);
  if (words.length === 0) return '_unknown';
  return words
    .map((w, i) => i === 0 ? w.toLowerCase() : w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
    .join('');
}

/** Convert any case to PascalCase. */
export function toPascalCase(s: string): string {
  const words = splitWords(s);
  if (words.length === 0) return '_Unknown';
  return words
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
    .join('');
}

/** Convert any case to SCREAMING_SNAKE_CASE. */
export function toScreamingSnake(s: string): string {
  const words = splitWords(s);
  if (words.length === 0) return '_UNKNOWN';
  return words.map((w) => w.toUpperCase()).join('_');
}

/** Convert any case to snake_case. */
export function toSnakeCase(s: string): string {
  const words = splitWords(s);
  if (words.length === 0) return '_unknown';
  return words.map((w) => w.toLowerCase()).join('_');
}

/** safeIdentifier + toCamelCase */
export function toCamelIdent(s: string): string {
  return toCamelCase(safeIdentifier(s));
}

/** safeIdentifier + toPascalCase */
export function toPascalIdent(s: string): string {
  return toPascalCase(safeIdentifier(s));
}

/** safeIdentifier + toScreamingSnake */
export function toScreamingSnakeIdent(s: string): string {
  return toScreamingSnake(safeIdentifier(s));
}
