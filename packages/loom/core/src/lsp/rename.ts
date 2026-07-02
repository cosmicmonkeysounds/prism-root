//! Cross-file beat rename, driven by the story graph.
//!
//! Renaming a beat touches its declaration line plus every divert that
//! resolves to it — across the whole project. The story graph already
//! carries exact `targetRange` anchors for each resolved edge, so the
//! rename is a batch of validated range splices per document; the
//! caller applies each document's `TextEdit[]` through its own save
//! path (the editor's `updateContents`, the relay's Loro doc, …).
//!
//! Not covered (yet): `visits(name)` / `played(name)` ledger queries
//! inside expressions, and prose mentions — those stay findable via
//! `referencesByName` for a manual pass.

import type { LoomFile } from "../parser/ast.ts";
import { EditError, type TextEdit } from "../parser/edit.ts";
import { parseDivertTarget } from "../parser/parser.ts";
import type { StoryGraph } from "./graph.ts";

/** The document surface `renameBeatEdits` reads (a `Workspace` slice). */
export interface RenameHost {
  docs: ReadonlyMap<string, { text: string; file: LoomFile }>;
  storyGraph(): StoryGraph;
}

/** A valid Loom beat identifier. */
const IDENT = /^[A-Za-z_][A-Za-z0-9_]*$/u;

/**
 * Compute the per-document edits that rename beat `key` (a graph key —
 * `lockdown` or `Owner.name`) to `newName`. Throws [`EditError`] when
 * the beat is unknown, the name is invalid or taken, or the beat is a
 * trait-derived projection (rename the template instead).
 */
export function renameBeatEdits(
  host: RenameHost,
  key: string,
  newName: string,
): Map<string, TextEdit[]> {
  if (!IDENT.test(newName)) {
    throw new EditError("notRenameable", `\`${newName}\` is not a valid beat name`);
  }
  const graph = host.storyGraph();
  const node = graph.beats.get(key);
  if (node === undefined) {
    throw new EditError("beatNotFound", `beat not found: ${key}`);
  }
  if (node.structural === "derived") {
    throw new EditError(
      "notRenameable",
      `\`${key}\` is a trait-shipped template instance — rename the template beat`,
    );
  }
  const newKey = node.owner === null ? newName : `${node.owner}.${newName}`;
  if (graph.beats.has(newKey)) {
    throw new EditError("nameTaken", `a beat named \`${newKey}\` already exists`);
  }
  if (node.uri === null || node.span === null) {
    throw new EditError("notRenameable", `\`${key}\` has no authored declaration site`);
  }

  const out = new Map<string, TextEdit[]>();
  const push = (uri: string, edit: TextEdit): void => {
    const list = out.get(uri);
    if (list) list.push(edit);
    else out.set(uri, [edit]);
  };
  /** Dedupe guard — one edit per (uri, start). */
  const seen = new Set<string>();

  // 1. The declaration line (`== name` / `beat name(…)`).
  {
    const doc = host.docs.get(node.uri);
    if (doc === undefined) {
      throw new EditError("staleAnchor", `document missing: ${node.uri}`);
    }
    const [ls, le] = lineBoundsIn(doc.text, node.span.start.offset);
    const line = doc.text.slice(ls, le);
    const at = tokenIndex(line, node.name);
    if (at < 0) {
      throw new EditError("staleAnchor", `declaration line lost \`${node.name}\``);
    }
    push(node.uri, { start: ls + at, end: ls + at + node.name.length, replacement: newName });
    seen.add(`${node.uri}@${ls + at}`);
  }

  // 2. Every resolved reference with an exact target anchor.
  for (const edge of graph.edges) {
    if (!edge.narrative || edge.to !== key || edge.targetRange === null) continue;
    const { uri, start, end } = edge.targetRange;
    if (seen.has(`${uri}@${start}`)) continue;
    const doc = host.docs.get(uri);
    if (doc === undefined) continue;
    const written = doc.text.slice(start, end);
    const rewritten = rewriteTargetName(written, node.name, newName);
    if (rewritten === null) continue; // anchor drifted — leave for a manual pass
    seen.add(`${uri}@${start}`);
    push(uri, { start, end, replacement: rewritten });
  }

  // 3. The `entry:` header property, when it names this beat.
  if (graph.entry === key) {
    for (const [uri, doc] of host.docs) {
      const prop = doc.file.header.properties.get("entry");
      if (prop === undefined || prop.value !== key) continue;
      const [ls, le] = lineBoundsIn(doc.text, prop.span.start.offset);
      const line = doc.text.slice(ls, le);
      const at = tokenIndex(line, key);
      if (at < 0 || seen.has(`${uri}@${ls + at}`)) continue;
      seen.add(`${uri}@${ls + at}`);
      push(uri, { start: ls + at, end: ls + at + key.length, replacement: newName });
    }
  }

  return out;
}

/**
 * Rewrite the name segment of a written divert target, preserving the
 * qualifier, its separator (`.` vs `/`), and any `#knot`. Returns null
 * when the written text no longer parses to the expected name.
 */
export function rewriteTargetName(
  written: string,
  oldName: string,
  newName: string,
): string | null {
  const t = parseDivertTarget(written);
  if (t.name !== oldName) return null;
  let head: string;
  if (t.qualifier === null) {
    head = newName;
  } else {
    const sep = written.startsWith(`${t.qualifier}/`) ? "/" : ".";
    head = `${t.qualifier}${sep}${newName}`;
  }
  return t.knot === null ? head : `${head}#${t.knot}`;
}

/** `[lineStart, lineEnd)` of the physical line containing `offset`. */
function lineBoundsIn(text: string, offset: number): [number, number] {
  const clamped = Math.min(offset, text.length);
  let start = clamped;
  while (start > 0 && text[start - 1] !== "\n") start -= 1;
  let end = clamped;
  while (end < text.length && text[end] !== "\n") end += 1;
  return [start, end];
}

/** First whole-token occurrence of `name` in `line`, or -1. */
function tokenIndex(line: string, name: string): number {
  let from = 0;
  for (;;) {
    const at = line.indexOf(name, from);
    if (at < 0) return -1;
    const before = at === 0 ? "" : line[at - 1]!;
    const afterIdx = at + name.length;
    const after = afterIdx >= line.length ? "" : line[afterIdx]!;
    const bound = (c: string) => c === "" || !/[0-9A-Za-z_]/.test(c);
    if (bound(before) && bound(after)) return at;
    from = at + 1;
  }
}
