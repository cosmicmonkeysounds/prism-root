//! C-style comment pre-pass (spec §6.1).
//!
//! Runs over raw `.loom` source before line classification and rewrites
//! every comment region to ASCII spaces (newlines preserved). Because
//! offsets and line numbers stay intact, every downstream span still
//! points at the right slice of the original source.
//!
//! Two flavours: `// …` to end of line and `/* … */` across any number
//! of lines. An opener (`//` or `/*`) is recognised only at the start of
//! a line or after whitespace, so `https://example.com` survives.
//! Comments are not recognised inside a ```` ``` ```` fence (spec §15).

import { Code, errorDiagnostic, type Diagnostic } from "./diagnostics.ts";
import { pos, span } from "./source.ts";
import { splitInclusive, stripPrefix, trimEndMatches } from "./rust.ts";

/** Strip comments out of `source`. Returns rewritten source + diagnostics. */
export function strip(source: string): [string, Diagnostic[]] {
  const diagnostics: Diagnostic[] = [];
  const out: string[] = [];
  let outLen = 0;
  const push = (s: string): void => {
    out.push(s);
    outLen += s.length;
  };

  // (line, col, offset) of the open `/*`, or null.
  let inBlock: { line: number; col: number; offset: number } | null = null;
  let inFence = false;
  let lineIdx = 0;
  let lineStartByte = 0;

  for (const rawLine of splitInclusive(source, "\n")) {
    // Fence handling — toggle on lines whose first non-whitespace tokens
    // are ```. Skipped mid-block-comment; a `/* */` that opens first wins.
    if (inBlock === null) {
      let trimmed = rawLine;
      let t = 0;
      while (t < trimmed.length && (trimmed[t] === " " || trimmed[t] === "\t")) t++;
      trimmed = trimmed.slice(t);
      const trimmedClean = trimEndMatches(trimEndMatches(trimmed, "\n"), "\r");
      const rest = stripPrefix(trimmedClean, "```");
      if (rest !== null) {
        if (inFence) {
          inFence = false;
        } else {
          const inlineClose = rest.endsWith("```") && rest.length >= 3;
          if (!inlineClose) inFence = true;
        }
        push(rawLine);
        lineIdx += 1;
        lineStartByte += rawLine.length;
        continue;
      }
    }

    if (inFence) {
      push(rawLine);
      lineIdx += 1;
      lineStartByte += rawLine.length;
      continue;
    }

    let i = 0;
    while (i < rawLine.length) {
      const c = rawLine[i]!;

      if (inBlock !== null) {
        if (c === "*" && rawLine[i + 1] === "/") {
          push(" ");
          push(" ");
          i += 2;
          inBlock = null;
        } else if (c === "\n" || c === "\r") {
          push(c);
          i += 1;
        } else {
          push(" ");
          i += 1;
        }
        continue;
      }

      if (c === "/" && rawLine[i + 1] === "/" && openerBoundaryOk(rawLine, i)) {
        // Line comment — replace through end of line, keep \n.
        while (i < rawLine.length && rawLine[i] !== "\n") {
          push(rawLine[i] === "\r" ? "\r" : " ");
          i += 1;
        }
        continue;
      }

      if (c === "/" && rawLine[i + 1] === "*" && openerBoundaryOk(rawLine, i)) {
        const col = i;
        inBlock = { line: lineIdx, col, offset: lineStartByte + col };
        push(" ");
        push(" ");
        i += 2;
        continue;
      }

      push(c);
      i += 1;
    }

    lineIdx += 1;
    lineStartByte += rawLine.length;
  }

  if (inBlock !== null) {
    const endByte = outLen;
    const endLine = lineIdx;
    const endCol = Math.max(0, endByte - lineStartByte);
    diagnostics.push(
      errorDiagnostic(
        Code.L1007UnterminatedBlockComment,
        span(pos(inBlock.line, inBlock.col, inBlock.offset), pos(endLine, endCol, endByte)),
        "block comment opened with `/*` is never closed",
      ),
    );
  }

  return [out.join(""), diagnostics];
}

/** `//` / `/*` are openers only at line start or after whitespace. */
function openerBoundaryOk(s: string, i: number): boolean {
  if (i === 0) return true;
  const prev = s[i - 1];
  return prev === " " || prev === "\t" || prev === "\n" || prev === "\r";
}
