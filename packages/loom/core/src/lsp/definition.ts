//! Go-to-definition for diverts (`-> beat`) and CHARACTER references
//! inside dialogue cues.

import { type GotoDefinitionResponse, type Location, type Position } from "./types.ts";
import { allChars, isAsciiUppercase } from "../parser/rust.ts";
import { lineAt, tokenAt } from "./util.ts";
import type { Workspace } from "./workspace.ts";

export function definitionAt(
  ws: Workspace,
  uri: string,
  pos: Position,
): GotoDefinitionResponse | null {
  const doc = ws.docs.get(uri);
  if (!doc) return null;
  const line = lineAt(doc.text, pos.line);
  const found = tokenAt(line, pos.character);
  if (!found) return null;
  const [token, start] = found;

  // Divert: `-> token` form.
  const arrow = line.slice(0, start).lastIndexOf("->");
  if (arrow >= 0) {
    if (line.slice(arrow + 2, start).trim() === "") {
      const entries = ws.beats.get(token);
      if (entries) {
        const locs: Location[] = entries.map((b) => ({ uri: b.uri, range: b.nameRange }));
        if (locs.length > 0) return locs;
      }
    }
  }

  // CHARACTER reference: ALL-CAPS token.
  if (allChars(token, (c) => isAsciiUppercase(c) || c === "_")) {
    const info = ws.characters.get(token);
    if (info) {
      return { uri: info.uri, range: info.range };
    }
  }

  return null;
}
