//! Effect lowering + directive parsing.
//!
//! Hook bodies are kept raw by the parser; `lowerRawBody` re-parses
//! them into structured `BodyItem`s so hooks and beats share one
//! executor. The directive helpers tokenise the small effect language
//! (`<set: …>`, `<capture: …>`, `<broadcast: …>`, …).

import type { BodyItem, RawLine } from "../../parser/index.ts";
import { parse } from "../../parser/index.ts";

/**
 * Re-parse a raw hook/declaration body into structured `BodyItem`s by
 * reconstructing it under a synthetic beat, so `<if:>`, dialogue,
 * directives, and `-> beat` diverts all lower the same as in a beat.
 */
export function lowerRawBody(lines: RawLine[]): BodyItem[] {
  if (lines.length === 0) return [];
  let minIndent = Infinity;
  for (const l of lines) minIndent = Math.min(minIndent, l.indent);
  const rows = ["== __hook"];
  for (const line of lines) {
    const indent = line.indent - minIndent + 2;
    rows.push(" ".repeat(indent) + line.text);
  }
  const [file] = parse(rows.join("\n") + "\n");
  for (const item of file.items) {
    if (item.kind === "beat" && item.value.name === "__hook") return item.value.body;
  }
  return [];
}

/** Split a directive's inner text into `verb` + `rest` on the first `:`. */
export function splitDirective(raw: string): { verb: string; rest: string } {
  const colon = raw.indexOf(":");
  if (colon < 0) return { verb: raw.trim(), rest: "" };
  return { verb: raw.slice(0, colon).trim(), rest: raw.slice(colon + 1).trim() };
}

export interface SetClause {
  path: string[];
  op: "=" | "+=" | "-=" | "*=" | "/=";
  rhs: string;
}

/** Parse `<set: path OP rhs>` body — `guest.score += 50`. */
export function parseSet(rest: string): SetClause | null {
  for (const op of ["+=", "-=", "*=", "/="] as const) {
    const i = rest.indexOf(op);
    if (i >= 0) {
      return { path: rest.slice(0, i).trim().split("."), op, rhs: rest.slice(i + op.length).trim() };
    }
  }
  const i = rest.indexOf("=");
  if (i >= 0 && rest[i + 1] !== "=") {
    return { path: rest.slice(0, i).trim().split("."), op: "=", rhs: rest.slice(i + 1).trim() };
  }
  return null;
}

/** Split `a <keyword> b` → `[a, b]`, trimming, or null if absent. */
export function splitKeyword(rest: string, keyword: string): [string, string] | null {
  const k = ` ${keyword} `;
  const i = rest.indexOf(k);
  if (i < 0) return null;
  return [rest.slice(0, i).trim(), rest.slice(i + k.length).trim()];
}
