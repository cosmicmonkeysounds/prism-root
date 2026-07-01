//! Pure, dependency-free helpers shared by the LSP request handlers and
//! the workspace index.
//!
//! These live apart from `workspace.ts` (where the Rust port keeps the
//! `span_to_range` / `line_at` family) so the handler modules and the
//! workspace can both import them without a runtime import cycle — the
//! handlers only need the `Workspace` *type*, which is erased.

import type { Diagnostic as ParserDiagnostic, Position as ParserPosition, Span } from "../parser/index.ts";
import { lines as splitLines } from "../parser/rust.ts";
import { type Diagnostic, DiagnosticSeverity, type Position, type Range } from "./types.ts";

/** A word character for token boundaries — `char::is_ascii_alphanumeric() || '_'`. */
function isWordChar(ch: string): boolean {
  return /[0-9A-Za-z_]/.test(ch);
}

/** Convert a parser [`Span`] to an LSP [`Range`]. */
export function spanToRange(span: Span): Range {
  return { start: positionToLsp(span.start), end: positionToLsp(span.end) };
}

/**
 * Convert a parser [`Position`] (line + UTF-16 column) to the LSP shape.
 * The parser's `column` is already a UTF-16 code-unit offset, which is
 * exactly what LSP `character` wants.
 */
export function positionToLsp(p: ParserPosition): Position {
  return { line: p.line, character: p.column };
}

/** Map a parser diagnostic into the LSP `Diagnostic` shape. */
export function toLspDiagnostic(d: ParserDiagnostic): Diagnostic {
  return {
    range: spanToRange(d.span),
    severity: d.severity === "error" ? DiagnosticSeverity.Error : DiagnosticSeverity.Warning,
    // `d.code` already IS the stable wire id (e.g. "L2001").
    code: d.code,
    source: "loom",
    message: d.message,
  };
}

/** Extract the line of text at zero-based `line`, or `""`. */
export function lineAt(text: string, line: number): string {
  return splitLines(text)[line] ?? "";
}

/**
 * Locate a `name` substring inside a span's source text so we can return
 * a tight range for go-to-definition / symbol selection. Falls back to
 * the span's range when the name isn't found on any covered line.
 */
export function nameRangeInText(text: string, span: Span, name: string): Range {
  const ls = splitLines(text);
  for (let lineIdx = span.start.line; lineIdx <= span.end.line; lineIdx++) {
    const line = ls[lineIdx];
    if (line === undefined) continue;
    // Whole-word match, not a raw substring: a short name (`OLE`) must not
    // resolve inside the leading keyword (`ROLE`) — take the first
    // token-boundary hit on the line.
    const spans = findTokenSpans(line, name);
    if (spans.length > 0) {
      const [start, end] = spans[0]!;
      return {
        start: { line: lineIdx, character: start },
        end: { line: lineIdx, character: end },
      };
    }
  }
  return spanToRange(span);
}

/**
 * Identify the alphanumeric / `_` token under the offset `col`, with its
 * `[token, start, end]` bounds. Returns `null` when the cursor is past
 * end-of-line or not sitting on a word.
 */
export function tokenAt(line: string, col: number): [string, number, number] | null {
  if (col > line.length) return null;
  let start = col;
  while (start > 0 && isWordChar(line[start - 1]!)) start -= 1;
  let end = col;
  while (end < line.length && isWordChar(line[end]!)) end += 1;
  if (start === end) return null;
  return [line.slice(start, end), start, end];
}

/**
 * Every `[start, end)` span on `line` where `needle` appears bounded by
 * non-identifier characters (whole-word matches only).
 */
export function findTokenSpans(line: string, needle: string): Array<[number, number]> {
  if (needle.length === 0 || needle.length > line.length) return [];
  const out: Array<[number, number]> = [];
  let i = 0;
  while (i + needle.length <= line.length) {
    if (line.slice(i, i + needle.length) === needle) {
      const beforeOk = i === 0 || !isWordChar(line[i - 1]!);
      const afterOk =
        i + needle.length === line.length || !isWordChar(line[i + needle.length]!);
      if (beforeOk && afterOk) {
        out.push([i, i + needle.length]);
        i += needle.length;
        continue;
      }
    }
    i += 1;
  }
  return out;
}

/** Re-export so callers that already hold a workspace can split lines. */
export { splitLines as lines };
