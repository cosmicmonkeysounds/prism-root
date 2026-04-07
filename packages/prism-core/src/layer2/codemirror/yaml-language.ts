/**
 * yaml-language.ts — YAML syntax highlighting for CodeMirror 6.
 *
 * Provides a StreamLanguage-based tokenizer that produces proper tokens
 * for all standard YAML constructs: keys, values, strings, numbers,
 * booleans, nulls, comments, anchors/aliases, tags, block scalars,
 * and structural punctuation.
 *
 * Generic Prism infrastructure — usable by any Prism app that edits YAML.
 *
 * Usage:
 *   import { yamlLanguageSupport } from '@prism/core/codemirror';
 *   extensions={[yamlLanguageSupport]}
 */

import {
  StreamLanguage,
  HighlightStyle,
  syntaxHighlighting,
  type StringStream,
  LanguageSupport,
} from "@codemirror/language";
import { tags } from "@lezer/highlight";

// ── State ────────────────────────────────────────────────────────────────────

interface YamlState {
  /** Inside a block scalar (| or >) — eat lines until dedented. */
  blockScalar: boolean;
  blockScalarIndent: number;
  /** After a colon we expect a value. */
  afterColon: boolean;
}

function startState(): YamlState {
  return { blockScalar: false, blockScalarIndent: -1, afterColon: false };
}

function copyState(s: YamlState): YamlState {
  return {
    blockScalar: s.blockScalar,
    blockScalarIndent: s.blockScalarIndent,
    afterColon: s.afterColon,
  };
}

// ── Tokenizer ────────────────────────────────────────────────────────────────

function token(stream: StringStream, state: YamlState): string | null {
  // ── Block scalar continuation ────────────────────────────────────────
  if (state.blockScalar) {
    const indent = stream.indentation();
    if (
      stream.sol() &&
      indent <= state.blockScalarIndent &&
      stream.peek() !== "\n"
    ) {
      state.blockScalar = false;
      state.blockScalarIndent = -1;
      // Fall through to normal parsing
    } else {
      stream.skipToEnd();
      return "string";
    }
  }

  // ── Blank line ───────────────────────────────────────────────────────
  if (stream.sol()) {
    state.afterColon = false;
  }

  // ── Whitespace ───────────────────────────────────────────────────────
  if (stream.eatSpace()) return null;

  // ── Comment ──────────────────────────────────────────────────────────
  if (stream.peek() === "#") {
    // YAML comments: # at start of token or after whitespace
    stream.skipToEnd();
    return "comment";
  }

  // ── Document markers ─────────────────────────────────────────────────
  if (stream.sol() && (stream.match("---") || stream.match("..."))) {
    // Only if it's the full line or followed by space/comment
    if (stream.eol() || stream.peek() === " " || stream.peek() === "#") {
      stream.skipToEnd();
      return "meta";
    }
  }

  // ── Anchor & alias ───────────────────────────────────────────────────
  if (stream.match(/^&[a-zA-Z_][a-zA-Z0-9_-]*/)) return "labelName";
  if (stream.match(/^\*[a-zA-Z_][a-zA-Z0-9_-]*/)) return "labelName";

  // ── Tag ──────────────────────────────────────────────────────────────
  if (stream.match(/^![a-zA-Z_!][a-zA-Z0-9_/.-]*/)) return "typeName";

  // ── Block scalar indicator ───────────────────────────────────────────
  if (stream.match(/^[|>][+-]?\d*/)) {
    state.blockScalar = true;
    state.blockScalarIndent = stream.indentation();
    stream.skipToEnd();
    return "meta";
  }

  // ── Strings ──────────────────────────────────────────────────────────
  if (stream.peek() === '"') {
    stream.next();
    while (!stream.eol()) {
      const ch = stream.next();
      if (ch === "\\") {
        stream.next();
        continue;
      }
      if (ch === '"') break;
    }
    return "string";
  }
  if (stream.peek() === "'") {
    stream.next();
    while (!stream.eol()) {
      const ch = stream.next();
      if (ch === "'" && stream.peek() === "'") {
        stream.next();
        continue;
      }
      if (ch === "'") break;
    }
    return "string";
  }

  // ── List item marker ─────────────────────────────────────────────────
  if (
    stream.match(/^- /) ||
    (stream.peek() === "-" && stream.string.trim() === "-")
  ) {
    return "punctuation";
  }

  // ── Key: value detection ─────────────────────────────────────────────
  // Colon followed by space, end of line, or opening flow collection
  if (stream.peek() === ":") {
    const next = stream.string.charAt(stream.pos + 1);
    if (
      next === " " ||
      next === "" ||
      next === "\n" ||
      next === "{" ||
      next === "["
    ) {
      stream.next();
      state.afterColon = true;
      return "punctuation";
    }
  }

  // ── Flow collection punctuation ──────────────────────────────────────
  if (stream.match(/^[{}\[\],]/)) return "bracket";

  // ── Values (after colon or in flow context) ──────────────────────────

  // Numbers
  if (
    stream.match(
      /^-?(?:0x[0-9a-fA-F]+|0o[0-7]+|0b[01]+|(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)\b/,
    )
  ) {
    return "number";
  }

  // Booleans and null (YAML 1.1 + 1.2)
  if (
    stream.match(
      /^(?:true|false|yes|no|on|off|True|False|Yes|No|On|Off|TRUE|FALSE|YES|NO|ON|OFF)\b/,
    )
  ) {
    return "bool";
  }
  if (stream.match(/^(?:null|Null|NULL|~)\b/)) {
    return "null";
  }

  // ── Plain scalar / key ───────────────────────────────────────────────
  // If we haven't seen a colon yet on this line, this might be a key
  if (!state.afterColon && stream.sol() || !state.afterColon) {
    // Attempt to match a key: consume up to an unquoted colon
    const rest = stream.string.slice(stream.pos);
    const colonIdx = rest.search(/:\s|:$/);
    if (colonIdx > 0 && !state.afterColon) {
      // This is a mapping key
      stream.pos += colonIdx;
      return "propertyName";
    }
  }

  // Fall through: consume a word as a plain scalar value
  if (stream.match(/^[^\s#:,\[\]{}'"&*!|>]+/)) {
    return state.afterColon ? "string" : "propertyName";
  }

  // Safety: advance one character to avoid infinite loops
  stream.next();
  return null;
}

// ── Language ─────────────────────────────────────────────────────────────────

export const yamlStreamLanguage = StreamLanguage.define<YamlState>({
  name: "yaml",
  startState,
  copyState,
  token,
  languageData: {
    commentTokens: { line: "#" },
  },
});

// ── Highlight style ──────────────────────────────────────────────────────────

/**
 * YAML highlight style — dark theme, matches Prism's zinc/violet palette.
 *
 * Maps StreamLanguage token names to CodeMirror highlight tags to colors.
 * The StreamLanguage tokenizer returns tag names (e.g. 'propertyName')
 * that map directly to @lezer/highlight tags.
 */
export const yamlHighlightStyle = HighlightStyle.define([
  { tag: tags.propertyName, color: "hsl(30 90% 65%)", fontWeight: "500" },
  { tag: tags.string, color: "hsl(120 50% 62%)" },
  { tag: tags.number, color: "hsl(35 90% 68%)" },
  { tag: tags.bool, color: "hsl(207 80% 68%)" },
  { tag: tags.null, color: "hsl(207 60% 58%)", fontStyle: "italic" },
  { tag: tags.comment, color: "hsl(220 15% 38%)", fontStyle: "italic" },
  { tag: tags.meta, color: "hsl(258 60% 72%)", fontWeight: "600" },
  { tag: tags.labelName, color: "hsl(192 80% 65%)" },
  { tag: tags.typeName, color: "hsl(45 80% 70%)" },
  { tag: tags.punctuation, color: "hsl(220 15% 55%)" },
  { tag: tags.bracket, color: "hsl(220 15% 55%)" },
]);

// ── Export ────────────────────────────────────────────────────────────────────

/**
 * Full YAML language support for CodeMirror 6.
 * Includes StreamLanguage tokenizer + highlight style.
 *
 * Drop-in extension for any Prism editor:
 *   extensions={[yamlLanguageSupport]}
 */
export const yamlLanguageSupport = new LanguageSupport(yamlStreamLanguage, [
  syntaxHighlighting(yamlHighlightStyle),
]);
