//! Hover information for directive verbs, CHARACTER refs, and beat
//! diverts. Resolution is purely lexical: we identify the token under
//! the cursor on the source line and look it up in the workspace index.

import { type Hover, type Position, type Range } from "./types.ts";
import { allChars, isAsciiUppercase } from "../parser/rust.ts";
import { DIRECTIVES } from "./completion.ts";
import { lineAt, tokenAt } from "./util.ts";
import type { Workspace } from "./workspace.ts";

export function hoverAt(ws: Workspace, uri: string, pos: Position): Hover | null {
  const doc = ws.docs.get(uri);
  if (!doc) return null;
  const line = lineAt(doc.text, pos.line);
  const found = tokenAt(line, pos.character);
  if (!found) return null;
  const [token, start, end] = found;
  const range: Range = {
    start: { line: pos.line, character: start },
    end: { line: pos.line, character: end },
  };

  // Directive head: `<name…` token immediately follows `<`.
  const open = line.slice(0, start).lastIndexOf("<");
  if (open >= 0) {
    if (line.slice(open + 1, start).trim() === "" && DIRECTIVES.includes(token)) {
      return markdown(`\`<${token}: …>\` — directive`, range);
    }
  }

  // Divert tail: `-> token` form.
  const arrow = line.slice(0, start).lastIndexOf("->");
  if (arrow >= 0) {
    if (line.slice(arrow + 2, start).trim() === "") {
      const entries = ws.beats.get(token);
      const beat = entries?.[0];
      if (beat) {
        let body = `**beat** \`${token}\``;
        if (beat.cast !== null) body += `\n\ncast: ${beat.cast}`;
        if (beat.setting !== null) body += `\n\nsetting: ${beat.setting}`;
        return markdown(body, range);
      }
    }
  }

  // TRAIT reference: a declared trait name (mixed-case, so it never collides
  // with the ALL-CAPS CHARACTER branch below).
  const trait = ws.traits.get(token);
  if (trait) {
    let body = `**TRAIT** \`${token}(${trait.params.join(", ")})\``;
    if (trait.beats.length > 0) body += `\n\nships: ${trait.beats.join(", ")}`;
    return markdown(body, range);
  }

  // CHARACTER reference: ALL-CAPS token that matches a declared name.
  if (allChars(token, (c) => isAsciiUppercase(c) || c === "_")) {
    const info = ws.characters.get(token);
    if (info) {
      let body = `**CHARACTER** \`${token}\``;
      if (info.mixins.length > 0) body += `\n\nis ${info.mixins.join(", ")}`;
      if (info.body.length > 0) {
        body += "\n\n```loom\n";
        for (const bodyLine of info.body) {
          body += bodyLine;
          body += "\n";
        }
        body += "```";
      }
      return markdown(body, range);
    }
  }

  return null;
}

function markdown(value: string, range?: Range): Hover {
  return {
    contents: { kind: "markdown", value },
    ...(range ? { range } : {}),
  };
}
