//! Line-oriented scanner for the Loom v3 surface.
//!
//! Walks source line-by-line and emits a `ScannedLine` for every
//! non-blank line, classified by its leading tokens so the parser can
//! stitch them into the AST without re-scanning characters.

import { declarationKindFromKeyword } from "./ast.ts";
import { strip } from "./comments.ts";
import { Code, errorDiagnostic, type Diagnostic } from "./diagnostics.ts";
import { pos, span, type Span } from "./source.ts";
import {
  allChars,
  isAsciiAlphanumeric,
  splitInclusive,
  splitTopLevelCommas,
  stripPrefix,
  stripSuffix,
} from "./rust.ts";

export type LineKind =
  | { kind: "heading"; title: string }
  | { kind: "property"; key: string; value: string }
  | { kind: "knotMarker"; name: string; params: string[] }
  | { kind: "sceneHeading"; text: string }
  | { kind: "letBinding"; name: string; expression: string }
  | { kind: "declarationOpener"; kindWord: string; name: string; mixin: string[] }
  | { kind: "choice"; sticky: boolean; text: string }
  | { kind: "divertLine"; text: string }
  | { kind: "tunnelReturn" }
  | { kind: "speaker"; text: string }
  | { kind: "parenthetical"; text: string }
  | { kind: "fence"; tail: string; inlineClose: boolean }
  | { kind: "directive"; text: string }
  | { kind: "prose"; text: string };

/** One classified non-blank source line. */
export interface ScannedLine {
  line: number;
  indent: number;
  text: string;
  startByte: number;
  endByte: number;
  kind: LineKind;
}

export function scannedLineSpan(l: ScannedLine): Span {
  return span(
    pos(l.line, l.indent, l.startByte),
    pos(l.line, l.indent + l.text.length, l.endByte),
  );
}

interface RawScanLine {
  line: number;
  indent: number;
  text: string;
  startByte: number;
  endByte: number;
}

/** Scan `source` into classified non-blank lines plus diagnostics. */
export function scan(source: string): [ScannedLine[], Diagnostic[]] {
  const [stripped, diagnostics] = strip(source);
  source = stripped;

  // Pass 1: split into non-blank physical lines with indent + spans.
  const raws: RawScanLine[] = [];
  let byte = 0;
  let lineIdx = 0;
  for (const raw of splitInclusive(source, "\n")) {
    const lineStartByte = byte;
    const lineLen = raw.length;
    byte += lineLen;

    let strippedLine = raw.endsWith("\n") ? raw.slice(0, raw.length - 1) : raw;
    strippedLine = strippedLine.endsWith("\r")
      ? strippedLine.slice(0, strippedLine.length - 1)
      : strippedLine;

    const [indent, contentOffset] = leadingIndent(strippedLine);
    let trimmed = strippedLine.slice(contentOffset);
    trimmed = trimmed.replace(/\s+$/u, "");
    if (trimmed.length === 0) {
      lineIdx += 1;
      continue;
    }

    if (strippedLine.slice(0, contentOffset).includes("\t")) {
      diagnostics.push(
        errorDiagnostic(
          Code.L1001TabIndent,
          span(
            pos(lineIdx, 0, lineStartByte),
            pos(lineIdx, contentOffset, lineStartByte + contentOffset),
          ),
          "indentation uses tabs; Loom v3 indents with spaces only",
        ),
      );
    }

    const startByte = lineStartByte + contentOffset;
    const endByte = startByte + trimmed.length;
    raws.push({ line: lineIdx, indent, text: trimmed, startByte, endByte });
    lineIdx += 1;
  }

  // Pass 2: stitch multi-line parentheticals, then classify.
  const lines: ScannedLine[] = [];
  let i = 0;
  while (i < raws.length) {
    const opener = raws[i]!;
    let text = opener.text;
    let endByte = opener.endByte;
    let consumed = 1;
    if (opener.text.startsWith("(") && parenDepth(opener.text) > 0) {
      let depth = parenDepth(opener.text);
      let lastLine = opener.line;
      let j = i + 1;
      while (depth > 0) {
        const cont = raws[j];
        if (cont === undefined) break;
        if (cont.line !== lastLine + 1 || cont.indent < opener.indent) break;
        text += " " + cont.text;
        depth += parenDepth(cont.text);
        endByte = cont.endByte;
        lastLine = cont.line;
        j += 1;
        consumed += 1;
      }
    }
    const kind = classify(text, opener.line, opener.startByte, diagnostics);
    lines.push({
      line: opener.line,
      indent: opener.indent,
      text,
      startByte: opener.startByte,
      endByte,
      kind,
    });
    i += consumed;
  }
  return [lines, diagnostics];
}

/** Net parenthesis depth — `(` count minus `)` count. */
function parenDepth(text: string): number {
  let depth = 0;
  for (const ch of text) {
    if (ch === "(") depth += 1;
    else if (ch === ")") depth -= 1;
  }
  return depth;
}

/** Leading-whitespace columns + the offset where content begins. */
function leadingIndent(line: string): [number, number] {
  let cols = 0;
  let off = 0;
  for (const ch of line) {
    if (ch === " " || ch === "\t") {
      cols += 1;
      off += ch.length;
    } else {
      break;
    }
  }
  return [cols, off];
}

function classify(
  text: string,
  line: number,
  startByte: number,
  diagnostics: Diagnostic[],
): LineKind {
  // Header heading: `# Title` (not `##`).
  {
    const rest = stripPrefix(text, "#");
    if (rest !== null && !rest.startsWith("#")) {
      return { kind: "heading", title: rest.trim() };
    }
  }

  // Knot marker: `== name`.
  {
    const rest = stripPrefix(text, "==");
    if (rest !== null) {
      const raw = rest.trim();
      const [name, params] = splitKnotNameAndParams(raw);
      if (name.length === 0) {
        diagnostics.push(
          errorDiagnostic(
            Code.L1004UnnamedKnot,
            lineSpan(line, startByte, text),
            "`==` knot marker is missing a name",
          ),
        );
      }
      return { kind: "knotMarker", name, params };
    }
  }

  // Scene heading: `INT.`, `EXT.`, …
  if (isSceneHeading(text)) {
    return { kind: "sceneHeading", text };
  }

  // `let name = expr` reactive binding.
  {
    const rest = stripPrefix(text, "let ");
    if (rest !== null) {
      const eq = rest.indexOf("=");
      if (eq >= 0) {
        return {
          kind: "letBinding",
          name: rest.slice(0, eq).trim(),
          expression: rest.slice(eq + 1).trim(),
        };
      }
    }
  }

  // Divert / tunnel-return.
  {
    const rest = stripPrefix(text, "->");
    if (rest !== null) {
      return { kind: "divertLine", text: rest.trim() };
    }
  }
  if (text === "<-") {
    return { kind: "tunnelReturn" };
  }

  // Choice — `*` or `+` followed by required whitespace + text.
  {
    const rest = stripPrefix(text, "*");
    if (rest !== null) {
      const body = stripPrefix(rest, " ");
      if (body !== null) {
        return { kind: "choice", sticky: false, text: body.replace(/^\s+/u, "") };
      }
      if (rest.length === 0) {
        diagnostics.push(
          errorDiagnostic(
            Code.L1003EmptyChoice,
            lineSpan(line, startByte, text),
            "`*` choice has no body text",
          ),
        );
        return { kind: "choice", sticky: false, text: "" };
      }
    }
  }
  {
    const rest = stripPrefix(text, "+");
    if (rest !== null) {
      const body = stripPrefix(rest, " ");
      if (body !== null) {
        return { kind: "choice", sticky: true, text: body.replace(/^\s+/u, "") };
      }
    }
  }

  // Whole-line angle-bracket directive — `<kind: args>` or `<kind>`.
  if (text.startsWith("<") && text.endsWith(">") && text.length >= 2 && text !== "<-") {
    const inner = text.slice(1, text.length - 1);
    return { kind: "directive", text: inner };
  }

  // Triple-backtick fence.
  {
    const rest = stripPrefix(text, "```");
    if (rest !== null) {
      const inlineClose = rest.endsWith("```") && rest.length >= 3;
      const tail = inlineClose ? rest.slice(0, rest.length - 3) : rest;
      return { kind: "fence", tail, inlineClose };
    }
  }

  // Declaration opener — `KEYWORD Name [is X, Y]`.
  {
    const opener = parseDeclarationOpener(text);
    if (opener !== null) {
      if (opener.name.length === 0) {
        diagnostics.push(
          errorDiagnostic(
            Code.L1006UnnamedDeclaration,
            lineSpan(line, startByte, text),
            `\`${opener.kindWord}\` declaration is missing a name`,
          ),
        );
      }
      if (opener.unterminatedMixin) {
        diagnostics.push(
          errorDiagnostic(
            Code.L1008UnterminatedMixinClause,
            lineSpan(line, startByte, text),
            `\`is\` clause must fit on one line — a wrapped clause is dropped`,
          ),
        );
      }
      return {
        kind: "declarationOpener",
        kindWord: opener.kindWord,
        name: opener.name,
        mixin: opener.mixin,
      };
    }
  }

  // Parenthetical line — the whole line is wrapped in `(…)`.
  {
    const inner = stripParens(text);
    if (inner !== null) {
      return { kind: "parenthetical", text: inner };
    }
  }

  // Property line — `key: value`.
  {
    const prop = propertySplit(text);
    if (prop !== null) {
      return { kind: "property", key: prop[0], value: prop[1] };
    }
  }

  // Speaker cue — pure ALL CAPS.
  if (isSpeakerLine(text)) {
    return { kind: "speaker", text };
  }

  return { kind: "prose", text };
}

function lineSpan(line: number, startByte: number, text: string): Span {
  return span(pos(line, 0, startByte), pos(line, text.length, startByte + text.length));
}

function isSceneHeading(text: string): boolean {
  const upper = text.replace(/^\s+/u, "");
  return (
    upper.startsWith("INT.") ||
    upper.startsWith("EXT.") ||
    upper.startsWith("INT/EXT") ||
    upper.startsWith("INT./EXT.") ||
    upper.startsWith("I/E ")
  );
}

function stripParens(text: string): string | null {
  const a = stripPrefix(text, "(");
  if (a === null) return null;
  const inner = stripSuffix(a, ")");
  if (inner === null) return null;
  // Reject `(x)y(z)` where parens aren't balanced as the outer wrapper.
  let depth = 0;
  for (let idx = 0; idx < inner.length; idx++) {
    const ch = inner[idx];
    if (ch === "(") {
      depth += 1;
    } else if (ch === ")") {
      if (depth === 0 && idx + 1 !== inner.length) {
        return null;
      }
      depth -= 1;
    }
  }
  return inner;
}

function propertySplit(text: string): [string, string] | null {
  const colon = text.indexOf(":");
  if (colon < 0) return null;
  const key = text.slice(0, colon);
  if (key.length === 0 || !allChars(key, isPropertyKeyChar)) return null;
  // Require whitespace (or EOL) after the colon — rejects URL-shaped lines.
  const after = text.slice(colon + 1);
  if (after.length > 0 && !/^\s/u.test(after)) return null;
  return [key.trim(), after.trim()];
}

function isPropertyKeyChar(ch: string): boolean {
  return isAsciiAlphanumeric(ch) || ch === "_" || ch === "-";
}

/** Split `ask_about(topic, NPC)` into `["ask_about", ["topic", "NPC"]]`. */
function splitKnotNameAndParams(raw: string): [string, string[]] {
  raw = raw.trim();
  const open = raw.indexOf("(");
  if (open >= 0) {
    const name = raw.slice(0, open).trim();
    const tail = raw.slice(open + 1);
    const close = tail.lastIndexOf(")");
    const inner = close >= 0 ? tail.slice(0, close) : tail;
    const params = inner
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    return [name, params];
  }
  return [raw, []];
}

function isSpeakerLine(text: string): boolean {
  let sawLetter = false;
  for (const ch of text) {
    if (ch >= "A" && ch <= "Z") {
      sawLetter = true;
    } else if ((ch >= "0" && ch <= "9") || ch === "_" || ch === " " || ch === "|") {
      // allowed
    } else {
      return false;
    }
  }
  return sawLetter;
}

interface DeclarationOpenerParse {
  kindWord: string;
  name: string;
  mixin: string[];
  /** True if the `is`-clause looks wrapped (trailing `,` / unbalanced `()`). */
  unterminatedMixin: boolean;
}

function parseDeclarationOpener(text: string): DeclarationOpenerParse | null {
  const m = /\s/u.exec(text);
  let kindWord: string;
  let rest: string;
  if (m) {
    kindWord = text.slice(0, m.index);
    rest = text.slice(m.index + m[0].length).trim();
  } else {
    kindWord = text;
    rest = "";
  }
  if (declarationKindFromKeyword(kindWord) === null) return null;

  const isIdx = rest.indexOf(" is ");
  let name: string;
  let mixinClause: string | null;
  if (isIdx >= 0) {
    name = rest.slice(0, isIdx).trim();
    mixinClause = rest.slice(isIdx + 4).trim();
  } else {
    name = rest;
    mixinClause = null;
  }

  // Split at top-level commas so a multi-arg trait application
  // (`CellWatch(loc: Internet, signal: lockdown)`) stays one entry — a comma
  // inside the argument list is not a separator between applications.
  const mixin =
    mixinClause !== null
      ? splitTopLevelCommas(mixinClause)
          .map((s) => s.trim())
          .filter((s) => s.length > 0)
      : [];

  // The `is`-clause is exactly one physical line (spec §2.2a). A trailing
  // top-level comma or unbalanced parens means it was wrapped — the remainder
  // would be silently dropped into the body, so flag it instead.
  let unterminatedMixin = false;
  if (mixinClause !== null) {
    const trimmed = mixinClause.trim();
    let depth = 0;
    for (const ch of trimmed) {
      if (ch === "(") depth += 1;
      else if (ch === ")") depth -= 1;
    }
    if (depth !== 0 || trimmed.endsWith(",")) unterminatedMixin = true;
  }

  return { kindWord, name, mixin, unterminatedMixin };
}
