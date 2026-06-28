//! Source positions and spans.
//!
//! Positions carry a zero-based line plus a JS-string offset (UTF-16
//! code units). `column` is the offset within the line; `offset` is the
//! offset from the start of the file. For ASCII source these match the
//! Rust parser's UTF-8 byte offsets exactly; the field is named
//! `offset` (not `byte`) because the unit is a JS string index.

export interface Position {
  line: number;
  column: number;
  offset: number;
}

export function pos(line: number, column: number, offset: number): Position {
  return { line, column, offset };
}

export const ZERO_POSITION: Position = { line: 0, column: 0, offset: 0 };

export interface Span {
  start: Position;
  end: Position;
}

export function span(start: Position, end: Position): Span {
  return { start, end };
}

export const ZERO_SPAN: Span = { start: ZERO_POSITION, end: ZERO_POSITION };

/** Span covering a single line from `start` to its end column/offset. */
export function lineSpanOf(start: Position, endOffset: number, endColumn: number): Span {
  return {
    start,
    end: { line: start.line, column: endColumn, offset: endOffset },
  };
}
