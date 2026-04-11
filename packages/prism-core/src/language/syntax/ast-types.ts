/**
 * AST types — Unist-compatible base node types, position tracking,
 * and source-mapping helpers.
 *
 * Used by scanners, parsers, and codegen throughout Prism.
 */

// ── Position & Range ─────────────────────────────────────────────────────────

export interface Position {
  offset: number;   // 0-based character index in source
  line: number;     // 1-based
  column: number;   // 0-based
}

export interface SourceRange {
  start: Position;
  end: Position;
}

// ── Unist-compatible base node ───────────────────────────────────────────────

export interface SyntaxNode {
  type: string;
  position?: SourceRange;
  children?: SyntaxNode[];
  value?: string;
  data?: Record<string, unknown>;
}

// ── Root node ────────────────────────────────────────────────────────────────

export interface RootNode extends SyntaxNode {
  type: 'root';
  children: SyntaxNode[];
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/** Build a Position from source string + offset. */
export function posAt(source: string, offset: number): Position {
  const lines = source.slice(0, offset).split('\n');
  const lastLine = lines[lines.length - 1] ?? '';
  return { offset, line: lines.length, column: lastLine.length };
}

/** Build a SourceRange from source string + start/end offsets. */
export function range(source: string, start: number, end: number): SourceRange {
  return { start: posAt(source, start), end: posAt(source, end) };
}
