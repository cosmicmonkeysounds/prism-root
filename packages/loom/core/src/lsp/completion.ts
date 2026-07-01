//! Completion suggestions for the Loom v3 surface.
//!
//! Context detection is line-local: we look at the prefix of the
//! current line up to the cursor and decide whether the user is in a
//! divert tail, a `<…>` directive head, an `is …` mixin list, or none
//! of the above. Spec §14 + §17 drive the candidate sets.

import { type CompletionItem, CompletionItemKind, type Position } from "./types.ts";
import { allChars, isAsciiAlphanumeric } from "../parser/rust.ts";
import { lineAt, lines } from "./util.ts";
import type { OpenDoc, Workspace } from "./workspace.ts";

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

  // Owner-qualified divert `-> self.` / `-> Owner.` — complete the owner's
  // owned + inherited beats. Checked before `isDivertPosition` (which bails on
  // the `.`) so a dotted divert tail routes here.
  const owner = ownerDivertPrefix(prefix);
  if (owner !== null) {
    const resolved = owner === "self" || owner === "SELF" || owner === "ME"
      ? enclosingOwner(doc, pos.line)
      : owner;
    const names = resolved !== null ? ws.ownerBeatNames(resolved) : [];
    return names.map((name) => simpleItem(name, CompletionItemKind.Function));
  }

  if (isDivertPosition(prefix)) {
    return [...ws.beats.keys()].map((name) => simpleItem(name, CompletionItemKind.Function));
  }

  if (isDirectivePosition(prefix)) {
    return DIRECTIVES.map((d) => simpleItem(d, CompletionItemKind.Keyword));
  }

  // Inside `is Trait(…)` — the args route the trait to beats, so complete
  // every global beat plus the enclosing character's owned + inherited beats.
  // Checked before `isIsPosition`, which would otherwise swallow the `(`.
  if (isTraitArgPosition(prefix)) {
    const candidates = new Set<string>(ws.beats.keys());
    const enclosing = enclosingOwner(doc, pos.line);
    if (enclosing !== null) for (const b of ws.ownerBeatNames(enclosing)) candidates.add(b);
    return [...candidates].map((name) => simpleItem(name, CompletionItemKind.Function));
  }

  if (isIsPosition(prefix)) {
    const out: CompletionItem[] = [...ws.characters.keys()].map((n) =>
      simpleItem(n, CompletionItemKind.Class),
    );
    for (const n of ws.traits.keys()) {
      out.push(simpleItem(n, CompletionItemKind.Interface));
    }
    // `SELF` / `ME` are reserved speaker tokens valid wherever a character
    // name is (spec redesign §9); offer them alongside the declared names.
    out.push(simpleItem("SELF", CompletionItemKind.Class));
    out.push(simpleItem("ME", CompletionItemKind.Class));
    return out;
  }

  return [];
}

/**
 * Nearest enclosing top-level declaration name (CHARACTER / ROLE / TRAIT),
 * found by scanning upward from `line` for a column-0 opener. Returns `null`
 * when the cursor sits outside any declaration.
 */
function enclosingOwner(doc: OpenDoc, line: number): string | null {
  const ls = lines(doc.text);
  const from = Math.min(line, ls.length - 1);
  for (let i = from; i >= 0; i--) {
    const text = ls[i] ?? "";
    if (text.length === 0 || /^\s/.test(text)) continue; // blank / indented — keep scanning up
    // The first column-0 line above the cursor decides: an opener means we are
    // inside its body; anything else (a top-level `==` beat, header, …) means
    // the cursor is outside every declaration.
    const m = /^(?:CHARACTER|ROLE|TRAIT)\s+([A-Za-z_][\w]*)/u.exec(text);
    return m ? m[1]! : null;
  }
  return null;
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

/**
 * True when the cursor sits inside the argument parens of a trait
 * application — `is Scanner(` with no closing `)` yet. The token before the
 * open paren must be an `is <Trait>` head, so a bare function-ish `foo(` in
 * prose doesn't trigger.
 */
export function isTraitArgPosition(prefix: string): boolean {
  const open = prefix.lastIndexOf("(");
  if (open < 0) return false;
  if (prefix.slice(open).includes(")")) return false;
  const head = prefix.slice(0, open);
  // The line must be a declaration opener carrying an `is` clause (`CHARACTER
  // Name is … Trait(`) — not a prose/dialogue line that merely contains
  // `is Word(`. The token just before `(` must be the applied trait name.
  return (
    /^(?:CHARACTER|ROLE|TRAIT)\s+[A-Za-z_]\w*\s+is\b/u.test(head) &&
    /[A-Za-z_][\w]*\s*$/u.test(head)
  );
}

/**
 * If the prefix is an owner-qualified divert tail `-> <owner>.<partial>`,
 * return the owner token (`self` / `SELF` / `ME` or a character/trait name);
 * otherwise `null`. The partial beat name after the dot may be empty.
 */
export function ownerDivertPrefix(prefix: string): string | null {
  const arrow = prefix.lastIndexOf("->");
  if (arrow < 0) return null;
  const tail = prefix.slice(arrow + 2).replace(/^\s+/, "");
  const m = /^([A-Za-z_][\w]*)\.(\w*)$/u.exec(tail);
  return m ? m[1]! : null;
}
