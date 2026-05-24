import { StreamLanguage, LanguageSupport, type StreamParser } from '@codemirror/language'

/**
 * Loom v3 CodeMirror language.
 *
 * Mirrors the Rust `loom_parser::lexer` line classifier (`packages/loom/parser/src/lexer.rs`)
 * and the TextMate grammar emitted by `loom_syntax::emit_tmgrammar`. The
 * patterns here are written so highlights are visually consistent with
 * the Zed / VSCode extensions.
 *
 * Diagnostics come from the real Rust parser via `loom-wasm`; see
 * `./loom-lint.ts`.
 */

const DECLARATION_KEYWORDS = [
  'CHARACTER',
  'TRAIT',
  'ITEM',
  'LOCATION',
  'FACTION',
  'STATS',
  'TREE',
  'GENERATOR',
  'SCENE',
  'COHORT',
] as const

const SYNTACTIC_DIRECTIVES = [
  'if',
  'else if',
  'else',
  'match',
  'case',
  'for',
  'each visit',
  'after',
  'otherwise',
  'anchor',
  'let',
] as const

const BUILTIN_DIRECTIVES = ['sfx', 'cue', 'pause', 'anchor', 'fire', 'set'] as const

const SCENE_HEADING_RE = /^(INT\.|EXT\.|INT\/EXT|INT\.\/EXT\.|I\/E\s+)/
const DECLARATION_RE = new RegExp(`^(${DECLARATION_KEYWORDS.join('|')})\\b`)
const SPEAKER_RE = /^[A-Z][A-Z0-9_ ]*(?:\|\s*[A-Z][A-Z0-9_ ]*)*$/
const PROPERTY_RE = /^([A-Za-z_][A-Za-z0-9_-]*)\s*:\s/
const CONTRACT_KEYS = new Set([
  'cast',
  'setting',
  'with topic',
  'entry',
  'tags',
  'title',
  'author',
  'next',
])

type LoomState = {
  inFence: boolean
}

const parser: StreamParser<LoomState> = {
  name: 'loom',

  startState() {
    return { inFence: false }
  },

  token(stream, state) {
    // ── inside a ``` fence ─────────────────────────────────────────
    if (state.inFence) {
      if (stream.sol() && stream.match(/```\s*$/)) {
        state.inFence = false
        return 'punctuation'
      }
      stream.skipToEnd()
      return 'comment'
    }

    // ── start-of-line dispatch ─────────────────────────────────────
    if (stream.sol()) {
      stream.eatSpace()
      const rest = stream.string.slice(stream.pos)

      // ```fence opener (possibly with a tag)
      if (stream.match(/```[a-zA-Z0-9_-]*\s*$/)) {
        state.inFence = true
        return 'punctuation'
      }

      // # Title (not ##)
      if (stream.match(/^#(?!#)\s*/)) {
        stream.skipToEnd()
        return 'header'
      }

      // == knot
      if (stream.match(/^==/)) {
        return 'def'
      }

      // Scene heading
      if (SCENE_HEADING_RE.test(rest)) {
        stream.skipToEnd()
        return 'header'
      }

      // * / + choice
      if (stream.match(/^[*+](?=\s)/)) {
        return 'keyword'
      }

      // -> divert / <- tunnel return
      if (stream.match(/^->/)) return 'keyword'
      if (stream.match(/^<-\s*$/)) return 'keyword'

      // let binding
      if (stream.match(/^let\s+/)) return 'def'

      // Declaration opener
      if (DECLARATION_RE.test(rest)) {
        const word = stream.match(DECLARATION_RE) as RegExpMatchArray | null
        if (word) return 'type'
      }

      // Whole-line parenthetical (performer cue)
      if (stream.match(/^\([^)]*\)\s*$/)) {
        return 'comment'
      }

      // ALL CAPS speaker cue — entire line
      if (SPEAKER_RE.test(rest)) {
        stream.skipToEnd()
        return 'tag'
      }

      // Property / contract key
      const propMatch = PROPERTY_RE.exec(rest)
      if (propMatch) {
        const key = propMatch[1]
        stream.match(/^[A-Za-z_][A-Za-z0-9_-]*/)
        return CONTRACT_KEYS.has(key) ? 'keyword' : 'property'
      }
    }

    // ── inline tokens ──────────────────────────────────────────────

    // <directive ...>
    if (stream.match(/^<[^>\n]+>/)) {
      const matched = stream.current()
      const inner = matched.slice(1, -1).split(':')[0].trim()
      if ((SYNTACTIC_DIRECTIVES as readonly string[]).includes(inner)) return 'keyword'
      if ((BUILTIN_DIRECTIVES as readonly string[]).includes(inner)) return 'builtin'
      return 'meta'
    }

    // {interpolation}
    if (stream.match(/^\{[^}\n]*\}/)) return 'variable'

    // [suppressed]
    if (stream.match(/^\[[^\]\n]*\]/)) return 'comment'

    // "string"
    if (stream.match(/^"(?:\\.|[^"\\])*"/)) return 'string'

    // number
    if (stream.match(/^-?\d+(?:\.\d+)?/)) return 'number'

    // is / with / END
    if (stream.match(/^\b(is|with|END)\b/)) return 'keyword'

    // true / false / nil
    if (stream.match(/^\b(true|false|nil)\b/)) return 'atom'

    // operators
    if (stream.match(/^[+\-*/%=<>!]+/)) return 'operator'

    if (stream.eatSpace()) return null
    stream.next()
    return null
  },

  languageData: {
    commentTokens: { line: '//' },
    indentOnInput: /^\s*[*+-]\s/,
  },
}

export function loomLanguage(): LanguageSupport {
  return new LanguageSupport(StreamLanguage.define(parser))
}
