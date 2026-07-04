//! Span-preserving, minimal-diff structural edits over `.loom` source.
//!
//! The parser keeps offset-accurate [`Span`]s on every AST node, and the
//! original source string is the single source of truth: the comment
//! pre-pass blanks comments to spaces rather than dropping them (see
//! `comments.ts`), so every offset still maps to the real text. That
//! lets us perform structural edits — reorder a beat, change a beat's
//! `cast:` line — as pure range splices that leave every untouched line
//! identical.
//!
//! Edits are returned as [`TextEdit`]s (independent range splices) rather
//! than a re-rendered file; [`applyEdits`] applies a batch. Port of the
//! Rust `loom_parser::edit` module — offsets here are JS string (UTF-16)
//! offsets (`Position.offset`) where the Rust uses UTF-8 byte offsets;
//! identical for ASCII source.

import type { Beat, BodyItem, Item, LoomFile } from "./ast.ts";
import { divertSpan } from "./ast.ts";
import type { Span } from "./source.ts";
import { parse } from "./parser.ts";

/**
 * A single range splice into the original source. `start..end` is a
 * half-open offset range into the source the edit was computed against;
 * `replacement` is the text to put there. A zero-length range
 * (`start === end`) is a pure insertion.
 */
export interface TextEdit {
  start: number;
  end: number;
  replacement: string;
}

/** Where to drop a beat when moving it. */
export type Anchor =
  | { kind: "before"; name: string }
  | { kind: "after"; name: string }
  | { kind: "start" }
  | { kind: "end" };

/** Why a structural edit could not be produced. */
export type EditErrorCode =
  | "beatNotFound"
  | "anchorNotFound"
  | "overlappingEdits"
  | "staleAnchor"
  | "itemNotFound"
  | "nameTaken"
  | "notRenameable";

export class EditError extends Error {
  readonly code: EditErrorCode;
  constructor(code: EditErrorCode, message: string) {
    super(message);
    this.name = "EditError";
    this.code = code;
  }
}

const beatNotFound = (n: string): EditError =>
  new EditError("beatNotFound", `beat not found: ${n}`);
const anchorNotFound = (n: string): EditError =>
  new EditError("anchorNotFound", `anchor beat not found: ${n}`);

/**
 * Apply a batch of non-overlapping [`TextEdit`]s to `source`, yielding
 * the new text. Edits are applied left-to-right over the original
 * offsets, so callers pass offsets relative to `source` (not to the
 * partially-edited result).
 */
export function applyEdits(source: string, edits: TextEdit[]): string {
  const sorted = [...edits].sort((a, b) => a.start - b.start || a.end - b.end);

  let prevEnd = 0;
  for (const e of sorted) {
    if (e.start < prevEnd) {
      throw new EditError("overlappingEdits", "overlapping text edits");
    }
    prevEnd = Math.max(e.end, e.start);
  }

  let out = "";
  let cursor = 0;
  for (const e of sorted) {
    out += source.slice(cursor, e.start);
    out += e.replacement;
    cursor = e.end;
  }
  out += source.slice(cursor);
  return out;
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

/**
 * Set a beat's contract property (`cast:`, `setting:`, …) to `value`.
 *
 * If the property already exists, only its line is rewritten (the
 * indentation and key are preserved; every other byte is untouched). If
 * absent, a new contract line is inserted after the last existing
 * contract property, or directly under the `==` opener when the contract
 * is empty. Returns an empty edit list when the value is already what was
 * requested.
 */
export function setBeatProperty(
  source: string,
  file: LoomFile,
  beat: string,
  key: string,
  value: string,
): TextEdit[] {
  const b = beatAt(file, beat);
  if (!b) throw beatNotFound(beat);

  const pv = b.contract.get(key);
  if (pv) {
    const [ls, le] = lineBounds(source, pv.span.start.offset);
    const indent = leadingWs(source.slice(ls, le));
    const replacement = `${indent}${key}: ${value}`;
    if (source.slice(ls, le) === replacement) return []; // no-op
    return [{ start: ls, end: le, replacement }];
  }

  // Property absent — insert a new contract line.
  let anchorEol: number;
  let indent: string;
  const entries = [...b.contract.entries()];
  const last = entries.at(-1);
  if (last) {
    const [ls, le] = lineBounds(source, last[1].span.start.offset);
    anchorEol = le;
    indent = leadingWs(source.slice(ls, le));
  } else {
    const [, le] = lineBounds(source, b.span.start.offset);
    anchorEol = le;
    indent = "  ";
  }

  if (anchorEol < source.length) {
    // `anchorEol` points at the line's trailing '\n'; insert a full new
    // line at the start of the following line.
    const start = anchorEol + 1;
    return [{ start, end: start, replacement: `${indent}${key}: ${value}\n` }];
  }
  // Anchor line is the last line in the file (no trailing newline).
  const start = source.length;
  return [{ start, end: start, replacement: `\n${indent}${key}: ${value}` }];
}

/**
 * Move the beat named `beat` to the position described by `anchor`.
 * Rewrites the source as a delete + insert pair; every other item's
 * bytes are preserved verbatim, only the seam separators normalise to a
 * blank line. Returns an empty edit list when the move is a no-op.
 */
export function moveBeat(
  source: string,
  file: LoomFile,
  beat: string,
  anchor: Anchor,
): TextEdit[] {
  const i = beatIndex(file, beat);
  if (i === null) throw beatNotFound(beat);
  const target = anchorIndex(file, anchor);
  const blocks = itemBlocks(source, file);
  return reorderBlockEdits(source, blocks, i, target);
}

/**
 * Insert a new (empty) `== name` beat at `anchor`. The new beat is
 * bracketed by blank lines; every existing line stays identical.
 */
export function insertBeat(
  source: string,
  file: LoomFile,
  name: string,
  anchor: Anchor,
): TextEdit[] {
  const target = anchorIndex(file, anchor);
  const blocks = itemBlocks(source, file);
  const ins = target < blocks.length ? blocks[target]![0] : source.length;
  const preceding = source.slice(0, ins);
  const trailing = trailingNewlines(preceding);
  let replacement = "";
  if (preceding.length > 0) replacement += "\n".repeat(Math.max(0, 2 - trailing));
  replacement += `== ${name}\n\n`;
  return [{ start: ins, end: ins, replacement }];
}

/**
 * Delete the beat named `beat` (its full block, including the trailing
 * separator up to the next item). Other items stay identical.
 */
export function removeBeat(source: string, file: LoomFile, beat: string): TextEdit[] {
  const i = beatIndex(file, beat);
  if (i === null) throw beatNotFound(beat);
  const blocks = itemBlocks(source, file);
  const [bs, be] = blocks[i]!;
  return [{ start: bs, end: be, replacement: "" }];
}

/**
 * Reorder a beat's body items: move the item at index `from` to index
 * `to` (final positions, 0-based over the beat's `body`). Returns an
 * empty edit list when out of range or a no-op.
 */
export function moveBodyItem(
  source: string,
  file: LoomFile,
  beat: string,
  from: number,
  to: number,
): TextEdit[] {
  const b = beatAt(file, beat);
  if (!b) throw beatNotFound(beat);
  const body = b.body;
  if (from >= body.length || to >= body.length || from === to) return [];
  const starts = body.map((it) => lineBounds(source, bodyItemSpan(it).start.offset)[0]);
  const regionEnd = Math.min(b.span.end.offset, source.length);
  const n = starts.length;
  const blocks: Array<[number, number]> = starts.map((s, k) => [
    s,
    k + 1 < n ? starts[k + 1]! : regionEnd,
  ]);
  // Translate "final index `to`" into the reorder helper's "insert before
  // original index" convention.
  const target = to > from ? to + 1 : to;
  return reorderBlockEdits(source, blocks, from, target);
}

// ---------------------------------------------------------------------------
// Graph-editor operations — the write side of the story-graph canvas.
// ---------------------------------------------------------------------------

/**
 * Validated exact-range replacement — the primitive under every
 * graph-side rewrite. `expect` is what the caller believes currently
 * occupies `start..end` (a divert target, a choice's text, …); a
 * mismatch means the anchor went stale (the buffer changed since the
 * graph was built) and the edit is refused rather than misapplied.
 */
export function replaceExact(
  source: string,
  start: number,
  end: number,
  expect: string,
  replacement: string,
): TextEdit[] {
  if (start < 0 || end > source.length || source.slice(start, end) !== expect) {
    throw new EditError("staleAnchor", `anchor no longer reads \`${expect}\``);
  }
  if (expect === replacement) return [];
  return [{ start, end, replacement }];
}

/**
 * Rewire a divert: replace the target text at `start..end` (the
 * `targetRange` the story graph computed) with `newTarget`, refusing a
 * stale anchor. Works for any divert form — plain, choice-nested,
 * tunnel call, owned-beat raw line — because the range is the exact
 * written target.
 */
export function retargetDivert(
  source: string,
  start: number,
  end: number,
  oldTarget: string,
  newTarget: string,
): TextEdit[] {
  return replaceExact(source, start, end, oldTarget, newTarget);
}

/**
 * Append a `-> target` divert as the last body line of `beat` — the
 * canvas's drag-a-connection edit. The line indents to match the last
 * body item (or two spaces under a bare opener).
 */
export function appendDivert(
  source: string,
  file: LoomFile,
  beat: string,
  target: string,
): TextEdit[] {
  return appendBodyLines(source, file, beat, [`-> ${target}`]);
}

/**
 * Append a choice option (`* text` / `+ text`), optionally already
 * wired to a target beat.
 */
export function appendChoice(
  source: string,
  file: LoomFile,
  beat: string,
  text: string,
  opts?: { sticky?: boolean; target?: string },
): TextEdit[] {
  const marker = opts?.sticky === true ? "+" : "*";
  const lines = [`${marker} ${text}`];
  if (opts?.target !== undefined) lines.push(`  -> ${opts.target}`);
  return appendBodyLines(source, file, beat, lines);
}

/**
 * Append raw body lines (already relative-indented among themselves) to
 * the end of `beat`'s block, before its trailing separator.
 */
export function appendBodyLines(
  source: string,
  file: LoomFile,
  beat: string,
  lines: string[],
): TextEdit[] {
  const b = beatAt(file, beat);
  if (!b) throw beatNotFound(beat);
  const i = beatIndex(file, beat)!;
  const blocks = itemBlocks(source, file);
  const [bs, be] = blocks[i]!;
  const content = source.slice(bs, be).replace(/\s+$/u, "");
  const insertAt = bs + content.length;

  // Match the indentation of the beat's last body item, defaulting to
  // two spaces directly under the `==` opener.
  let indent = "  ";
  const last = b.body.at(-1);
  if (last !== undefined) {
    const [ls, le] = lineBounds(source, bodyItemSpan(last).start.offset);
    indent = leadingWs(source.slice(ls, le));
    if (indent.length === 0) indent = "  ";
  }
  const replacement = lines.map((l) => `\n${indent}${l}`).join("");
  return [{ start: insertAt, end: insertAt, replacement }];
}

/**
 * Insert raw body lines (already relative-indented among themselves)
 * *before* the top-level body item at (0-based) `index` in `beat`.
 * `index >= body.length` appends. The lines indent to match the item
 * they're inserted before (two spaces under a bare opener), so the
 * node editor can compose a beat top-down, not just append.
 */
export function insertBodyLines(
  source: string,
  file: LoomFile,
  beat: string,
  index: number,
  lines: string[],
): TextEdit[] {
  const b = beatAt(file, beat);
  if (!b) throw beatNotFound(beat);
  if (index < 0) throw new EditError("itemNotFound", `no body item at index ${index}`);
  if (index >= b.body.length) return appendBodyLines(source, file, beat, lines);
  const anchor = b.body[index]!;
  const [ls, le] = lineBounds(source, bodyItemSpan(anchor).start.offset);
  let indent = leadingWs(source.slice(ls, le));
  if (indent.length === 0) indent = "  ";
  const replacement = lines.map((l) => `${indent}${l}\n`).join("");
  return [{ start: ls, end: ls, replacement }];
}

/**
 * Append a top-level declaration block (`CHARACTER Name`, `LOCATION Name`,
 * …) at the end of the file, bracketed by a blank line. `props` become
 * indented `key: value` lines. The canvas's "new entity" edit — the
 * declaration parses back into the same node the graph would draw for it.
 */
export function appendDeclaration(
  source: string,
  kind: string,
  name: string,
  props?: Record<string, string>,
): TextEdit[] {
  const trailing = trailingNewlines(source);
  let replacement = "";
  if (source.length > 0) replacement += "\n".repeat(Math.max(0, 2 - trailing));
  replacement += `${kind} ${name}\n`;
  for (const [k, v] of Object.entries(props ?? {})) {
    replacement += `  ${k}: ${v}\n`;
  }
  const at = source.length;
  return [{ start: at, end: at, replacement }];
}

/**
 * Delete the body item at (0-based) `index` in `beat` — its whole line
 * block, including nested content, up to the next sibling item.
 */
export function removeBodyItem(
  source: string,
  file: LoomFile,
  beat: string,
  index: number,
): TextEdit[] {
  const b = beatAt(file, beat);
  if (!b) throw beatNotFound(beat);
  if (index < 0 || index >= b.body.length) {
    throw new EditError("itemNotFound", `no body item at index ${index}`);
  }
  const starts = b.body.map((it) => lineBounds(source, bodyItemSpan(it).start.offset)[0]);
  const regionEnd = Math.min(b.span.end.offset, source.length);
  const end = index + 1 < starts.length ? starts[index + 1]! : regionEnd;
  return [{ start: starts[index]!, end, replacement: "" }];
}

/**
 * Rename a top-level beat's declaration line (`== old` → `== new`).
 * Reference updates live at the workspace layer (`lsp/rename.ts`),
 * which sees every file; this op is the declaration site only.
 */
export function renameBeatDecl(
  source: string,
  file: LoomFile,
  oldName: string,
  newName: string,
): TextEdit[] {
  const b = beatAt(file, oldName);
  if (!b) throw beatNotFound(oldName);
  const [ls, le] = lineBounds(source, b.span.start.offset);
  const line = source.slice(ls, le);
  const at = line.indexOf(oldName);
  if (at < 0) throw new EditError("staleAnchor", `opener line lost \`${oldName}\``);
  return replaceExact(source, ls + at, ls + at + oldName.length, oldName, newName);
}

// ---------------------------------------------------------------------------
// Convenience wrappers — parse + edit + apply, mirroring the wasm surface
// the editor used to call (`apply_beat_property` / `apply_move_beat` / …).
// ---------------------------------------------------------------------------

export function applyBeatProperty(
  source: string,
  beat: string,
  key: string,
  value: string,
): string {
  const [file] = parse(source);
  return applyEdits(source, setBeatProperty(source, file, beat, key, value));
}

export function applyMoveBeat(
  source: string,
  beat: string,
  anchorKind: string,
  anchorName: string,
): string {
  const [file] = parse(source);
  return applyEdits(source, moveBeat(source, file, beat, anchorFromParts(anchorKind, anchorName)));
}

export function applyInsertBeat(
  source: string,
  name: string,
  anchorKind: string,
  anchorName: string,
): string {
  const [file] = parse(source);
  return applyEdits(source, insertBeat(source, file, name, anchorFromParts(anchorKind, anchorName)));
}

export function applyRemoveBeat(source: string, beat: string): string {
  const [file] = parse(source);
  return applyEdits(source, removeBeat(source, file, beat));
}

function anchorFromParts(kind: string, name: string): Anchor {
  switch (kind) {
    case "before":
      return { kind: "before", name };
    case "after":
      return { kind: "after", name };
    case "start":
      return { kind: "start" };
    case "end":
      return { kind: "end" };
    default:
      throw new EditError("anchorNotFound", `unknown anchor kind: ${kind}`);
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Resolve an [`Anchor`] to a target index in `file.items`. */
function anchorIndex(file: LoomFile, anchor: Anchor): number {
  switch (anchor.kind) {
    case "start":
      return 0;
    case "end":
      return file.items.length;
    case "before": {
      const i = beatIndex(file, anchor.name);
      if (i === null) throw anchorNotFound(anchor.name);
      return i;
    }
    case "after": {
      const i = beatIndex(file, anchor.name);
      if (i === null) throw anchorNotFound(anchor.name);
      return i + 1;
    }
  }
}

/**
 * Move block `i` to just before block `target` (`target === len` => end).
 * Returns `[]` for a no-op. Shared by `moveBeat` / `moveBodyItem`;
 * `blocks` partition the editable region.
 */
function reorderBlockEdits(
  source: string,
  blocks: Array<[number, number]>,
  i: number,
  target: number,
): TextEdit[] {
  const n = blocks.length;
  if (i >= n || target === i || target === i + 1) return [];
  const [bs, be] = blocks[i]!;
  const content = source.slice(bs, be).trimEnd();
  const moved = `${content}\n\n`;
  const del: TextEdit = { start: bs, end: be, replacement: "" };
  const ins = target < n ? blocks[target]![0] : source.length;
  const preceding = source.slice(0, ins);
  const trailing = trailingNewlines(preceding);
  let replacement = "";
  if (preceding.length > 0) replacement += "\n".repeat(Math.max(0, 2 - trailing));
  replacement += moved;
  const insert: TextEdit = { start: ins, end: ins, replacement };
  return [del, insert];
}

function beatIndex(file: LoomFile, name: string): number | null {
  const idx = file.items.findIndex((it) => it.kind === "beat" && it.value.name === name);
  return idx < 0 ? null : idx;
}

function beatAt(file: LoomFile, name: string): Beat | null {
  for (const it of file.items) {
    if (it.kind === "beat" && it.value.name === name) return it.value;
  }
  return null;
}

/** The source span of any top-level item. */
function itemSpan(item: Item): Span {
  return item.value.span;
}

/**
 * Offset of the physical line that contains the item, snapped back to the
 * start of that line.
 */
function itemStart(source: string, item: Item): number {
  return lineBounds(source, itemSpan(item).start.offset)[0];
}

/**
 * Contiguous `[start, end)` ranges, one per top-level item, that
 * partition the source from the first item to EOF. Each block carries the
 * trailing separator up to the next item.
 */
function itemBlocks(source: string, file: LoomFile): Array<[number, number]> {
  const starts = file.items.map((it) => itemStart(source, it));
  const n = starts.length;
  return starts.map((s, k) => [s, k + 1 < n ? starts[k + 1]! : source.length]);
}

/** The source span of a beat body item, across every `BodyItem` kind. */
function bodyItemSpan(item: BodyItem): Span {
  return item.kind === "divert" ? divertSpan(item.value) : item.value.span;
}

/**
 * `[lineStart, lineEnd)` offset range of the physical line containing
 * `offset`, where `lineEnd` excludes the trailing '\n'.
 */
function lineBounds(source: string, offset: number): [number, number] {
  const clamped = Math.min(offset, source.length);
  let start = clamped;
  while (start > 0 && source[start - 1] !== "\n") start -= 1;
  let end = clamped;
  while (end < source.length && source[end] !== "\n") end += 1;
  return [start, end];
}

/** Leading run of spaces/tabs on a single line (no trailing '\n'). */
function leadingWs(line: string): string {
  let end = 0;
  while (end < line.length && (line[end] === " " || line[end] === "\t")) end += 1;
  return line.slice(0, end);
}

/** Count of trailing `\n` characters at the end of `s`. */
function trailingNewlines(s: string): number {
  let count = 0;
  for (let k = s.length - 1; k >= 0 && s[k] === "\n"; k -= 1) count += 1;
  return count;
}
