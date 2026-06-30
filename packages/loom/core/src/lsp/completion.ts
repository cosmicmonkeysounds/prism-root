//! Completion suggestions for the Loom v3 surface.
//!
//! Context detection is line-local: we look at the prefix of the
//! current line up to the cursor and decide whether the user is in a
//! divert tail, a `<…>` directive head, an `is …` mixin list, or none
//! of the above. Spec §14 + §17 drive the candidate sets.

import { type CompletionItem, CompletionItemKind, type Position } from "./types.ts";
import { allChars, isAsciiAlphanumeric } from "../parser/rust.ts";
import { lineAt } from "./util.ts";
import type { Workspace } from "./workspace.ts";

/**
 * Canonical directive verbs (spec §14). Hard-coded here, mirroring the
 * Rust LSP: the runtime registry isn't reachable from the parser-only
 * language surface (and shouldn't be).
 */
export const DIRECTIVES: readonly string[] = [
  "sfx",
  "cue",
  "set",
  "fire",
  "pause",
  "shuffle",
  "cycle",
  "spawn",
  "cancel",
  "goal",
  "broadcast",
  "enroll",
  "goto",
  "compose",
  "flash",
  "heal",
  "if",
  "else",
  "else if",
  "match",
  "for",
  "each visit",
  "after",
  "otherwise",
  "anchor",
  "let",
];

export function completionAt(ws: Workspace, uri: string, pos: Position): CompletionItem[] {
  const doc = ws.docs.get(uri);
  if (!doc) return [];
  const line = lineAt(doc.text, pos.line);
  const col = pos.character;
  const prefix = col <= line.length ? line.slice(0, col) : line;

  if (isDivertPosition(prefix)) {
    return [...ws.beats.keys()].map((name) => simpleItem(name, CompletionItemKind.Function));
  }

  if (isDirectivePosition(prefix)) {
    return DIRECTIVES.map((d) => simpleItem(d, CompletionItemKind.Keyword));
  }

  if (isIsPosition(prefix)) {
    const out: CompletionItem[] = [...ws.characters.keys()].map((n) =>
      simpleItem(n, CompletionItemKind.Class),
    );
    for (const n of ws.traits.keys()) {
      out.push(simpleItem(n, CompletionItemKind.Interface));
    }
    return out;
  }

  return [];
}

function simpleItem(label: string, kind: CompletionItemKind): CompletionItem {
  return { label, kind };
}

/**
 * True when the prefix ends with `->` (optionally followed by an
 * identifier-in-progress and arbitrary whitespace).
 */
export function isDivertPosition(prefix: string): boolean {
  const idx = prefix.lastIndexOf("->");
  if (idx < 0) return false;
  const tail = prefix.slice(idx + 2);
  return allChars(
    tail,
    (c) => /\s/.test(c) || isAsciiAlphanumeric(c) || c === "_" || c === "/" || c === "#",
  );
}

/**
 * True when the cursor sits inside an unterminated `<…>` directive head:
 * the last `<` on the line has no matching `>` after it.
 */
export function isDirectivePosition(prefix: string): boolean {
  const open = prefix.lastIndexOf("<");
  if (open < 0) return false;
  return !prefix.slice(open).includes(">");
}

/**
 * True when the line is an `is ` mixin clause and the cursor is past the
 * `is ` keyword.
 */
export function isIsPosition(prefix: string): boolean {
  const trimmed = prefix.replace(/^\s+/, "");
  if (trimmed.startsWith("is ")) {
    const rest = trimmed.slice(3);
    return allChars(rest, (c) => c !== "<" && c !== ">");
  }
  const idx = prefix.lastIndexOf(" is ");
  if (idx >= 0) {
    const tail = prefix.slice(idx + 4);
    return allChars(tail, (c) => c !== "<" && c !== ">");
  }
  return false;
}
