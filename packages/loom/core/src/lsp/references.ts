//! Find-references: every occurrence of the identifier under the cursor
//! across every open document in the workspace.
//!
//! v1 is identifier-shaped: it does not distinguish a `WREN` dialogue
//! cue from a CHARACTER declaration named `WREN` — both tokens land in
//! the result set. The editor renders the matches grouped by URI, which
//! is enough signal for navigation; semantic filtering can layer on top
//! once the workspace index grows per-kind tables.

import { type Location, type Position } from "./types.ts";
import { findTokenSpans, lineAt, lines, tokenAt } from "./util.ts";
import type { Workspace } from "./workspace.ts";

export function referencesAt(ws: Workspace, uri: string, pos: Position): Location[] {
  const doc = ws.docs.get(uri);
  if (!doc) return [];
  const line = lineAt(doc.text, pos.line);
  const found = tokenAt(line, pos.character);
  if (!found) return [];
  const needle = found[0];

  const out: Location[] = [];
  for (const [docUri, openDoc] of ws.docs) {
    lines(openDoc.text).forEach((lineText, lineNo) => {
      for (const [start, end] of findTokenSpans(lineText, needle)) {
        out.push({
          uri: docUri,
          range: {
            start: { line: lineNo, character: start },
            end: { line: lineNo, character: end },
          },
        });
      }
    });
  }
  return out;
}
