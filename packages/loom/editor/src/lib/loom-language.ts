import { StreamLanguage, LanguageSupport, type StreamParser } from '@codemirror/language'
import type { StringStream } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'
import {
  DECLARATIONS,
  SYNTACTIC_DIRECTIVES,
  CONTRACT_KEYS,
  RESERVED_INLINE,
  LIVE_KEYWORDS,
  SIMULACRA_KEYWORDS,
  MERIDIAN_KEYWORDS,
} from '@loom/core/parser'

/**
 * Loom v3 CodeMirror language.
 *
 * Mirrors the `@loom/core` line classifier (`core/src/parser/lexer.ts`) and
 * the TextMate grammar emitted by `loom_syntax::emit_tmgrammar`. The keyword
 * tables are imported straight from `@loom/core/parser` — `keywords.ts` is the
 * single source of truth for the parser AND every editor surface, so the
 * highlighter can never drift out of sync with the language again.
 *
 * The tokenizer is a small per-line state machine. Every line is classified at
 * its start into a `mode` that governs how the rest of the line is tokenised;
 * `<…>` directives and `{…}` interpolations push a nested `ctx`. The crucial
 * invariant (spec §14.3): reserved words like `is` / `with` / `END` / `self`
 * are keywords ONLY inside expression contexts — never in prose/dialogue, so
 * "the room *is* the network" stays plain prose.
 *
 * Diagnostics come from the native TypeScript parser (`@loom/core`); see
 * `./loom-lint.ts`.
 *
 * Token names map through `tokenTable` to precise `@lezer/highlight` tags that
 * the one-dark theme colours reliably: kw→violet, type→yellow, label→blue,
 * fn→blue, prop→coral, num→yellow, str→green, atom/speaker/interp→orange,
 * op→cyan, comment/meta→grey, heading→coral-bold.
 */

// ── token → tag table ────────────────────────────────────────────────
const tokenTable = {
  kw: t.keyword,
  type: t.typeName,
  label: t.labelName,
  fn: t.function(t.variableName),
  prop: t.propertyName,
  num: t.number,
  str: t.string,
  atom: t.atom,
  op: t.operator,
  comment: t.comment,
  meta: t.meta,
  heading: t.heading,
  speaker: t.special(t.variableName),
  interp: t.special(t.variableName),
  def: t.variableName,
}

// ── keyword tables (imported from @loom/core) ────────────────────────
const DECLARATION_RE = new RegExp(`^(${[...DECLARATIONS].join('|')})(?=\\s|$)`)
const CONTRACT_SET = new Set<string>(CONTRACT_KEYS)

/** Slot-type structural words highlighted inside a property *value*
 *  (typed slots, spec §8) — a tight set, so a prose value only ever
 *  mis-highlights a stray `to` / `of`. Reserved words are excluded. */
const SLOT_WORDS = ['list', 'map', 'range', 'any', 'of', 'to', 'bool', 'int', 'float', 'text']

/** Live / hook keywords that lead or punctuate an `on …` hook trigger. */
const HOOK_WORDS = new Set<string>([
  'enters', 'exits', 'joins', 'passes', 'drops', 'below', 'reaches',
  'every', 'at', 'when', 'complete', 'fail', 'boot', 'none',
])

/** Keywords that open a body line (no colon) inside a STATS / TREE / SCENE
 *  / GENERATOR / CHARACTER body. Matched case-sensitively at line start, so
 *  a capitalised prose sentence never trips them. Highlighting them turns
 *  the whole line into an expression (numbers / operators light up). */
const BODY_OPENER_RE = wordAlternation(
  dedupeSortDesc([
    // Meridian primitives (spec §11).
    'attribute', 'axis', 'pool', 'stat', 'node',
    // Simulacra / coroutine openers (spec §10).
    'goal', 'generator', 'beat', 'method', 'init', 'fill',
    'trusts', 'respects', 'fears', 'reacts', 'knows', 'mirror',
    'spawn', 'run', 'wait', 'yield', 'loop', 'return', 'every', 'when', 'at',
  ]),
)

/** Field keys whose value is an expression even without a leading digit
 *  (`curve: level * level * 50`, `effect: stat(damage) += 5`). */
const FIELD_EXPR_KEYS = new Set<string>([
  'curve', 'effect', 'requires', 'max', 'regen', 'cost',
])

/** Every keyword that may appear inside an expression context, longest
 *  first so multi-word entries (`else if`, `active when`) win. */
const EXPR_KEYWORDS = dedupeSortDesc([
  ...RESERVED_INLINE,
  ...LIVE_KEYWORDS,
  ...SIMULACRA_KEYWORDS,
  ...MERIDIAN_KEYWORDS,
  ...SYNTACTIC_DIRECTIVES,
  ...SLOT_WORDS,
  'and', 'but', 'not', 'in',
])
const EXPR_KEYWORD_RE = wordAlternation(EXPR_KEYWORDS)

/** The subset legal inside a property value (no reserved / control words). */
const VALUE_KEYWORD_RE = wordAlternation(dedupeSortDesc(SLOT_WORDS))

/** Directive verbs whose *shape* is grammar (highlight violet, like `if`). */
const SYNTACTIC_RE = wordAlternation(dedupeSortDesc([...SYNTACTIC_DIRECTIVES]))

function dedupeSortDesc(words: string[]): string[] {
  return [...new Set(words)].sort((a, b) => b.length - a.length)
}

/** Build `/^(a|b|c)\b/` matching any word, spaces collapsed to `\s+`,
 *  entries pre-sorted longest-first by the caller. */
function wordAlternation(words: string[]): RegExp {
  const alt = words
    .map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&').replace(/\s+/g, '\\s+'))
    .join('|')
  return new RegExp(`^(?:${alt})\\b`)
}

const SCENE_HEADING_RE = /^(INT\.|EXT\.|INT\/EXT|INT\.\/EXT\.|I\/E\s+)/
const SPEAKER_RE = /^[A-Z][A-Z0-9_ ]*(?:\|\s*[A-Z][A-Z0-9_ ]*)*$/
const PROPERTY_RE = /^([A-Za-z_][A-Za-z0-9_-]*)\s*:(?=\s|$)/

/** Multi-word field / contract keys (`active when:`, `completes when:`,
 *  `with topic:`) — `PROPERTY_RE` only handles single-word keys. */
const MULTIWORD_KEYS = dedupeSortDesc(
  [...CONTRACT_KEYS, ...SIMULACRA_KEYWORDS].filter((k) => /\s/.test(k)),
)
const MULTIWORD_KEY_RE = new RegExp(
  `^(?:${MULTIWORD_KEYS.map((k) => k.replace(/\s+/g, '\\s+')).join('|')})\\s*:`,
)
const IDENT_RE = /^[A-Za-z_][A-Za-z0-9_]*/
const NUMBER_RE = /^-?\d+(?:\.\d+)?/
const OPERATOR_RE = /^(?::=|->|[+\-*/%=<>!|?,:().])/

// ── per-line modes / nested contexts ─────────────────────────────────
type Mode = 'prose' | 'value' | 'expr' | 'hook' | 'decl' | 'knot' | 'divert'
type Ctx = 'directive' | 'interp'
/** Which enclosing block a line sits in — the tokenizer is line-local, so
 *  this is tracked across lines to mirror the parser's grammar: `on …`
 *  hooks and body-opener keywords (`stat`, `wait`, …) are keywords ONLY
 *  inside a declaration body, never in a `== knot` beat's dialogue. */
type Block = 'top' | 'decl' | 'beat'

type LoomState = {
  inFence: boolean
  inBlockComment: boolean
  mode: Mode
  /** enclosing block, persisted across lines (see `Block`). */
  block: Block
  /** nested `<…>` / `{…}` contexts (a `{…}` may sit inside a `<…>`). */
  ctxStack: Ctx[]
  /** directive verb already consumed for the current `<…>`? */
  verbSeen: boolean
  /** for a whole-line `<…>` the closer is the LAST `>` on the line (so a
   *  `>=` / `>` comparison inside stays part of the expression); an inline
   *  directive in prose closes at the first `>`. Mirrors the lexer. */
  dirLastCloser: boolean
  /** declaration name / knot name / divert target already consumed? */
  headSeen: boolean
}

type StreamLike = { pos: number; string: string }

/** `//` / `/*` only open comments at line start or after whitespace —
 *  mirrors the Rust pre-pass in `loom_parser::comments`, so `https://…`
 *  stays prose. */
function commentBoundaryOk(stream: StreamLike): boolean {
  if (stream.pos === 0) return true
  const prev = stream.string.charAt(stream.pos - 1)
  return prev === ' ' || prev === '\t'
}

const parser: StreamParser<LoomState> = {
  name: 'loom',
  tokenTable,

  startState() {
    return {
      inFence: false,
      inBlockComment: false,
      mode: 'prose',
      block: 'top',
      ctxStack: [],
      verbSeen: false,
      dirLastCloser: false,
      headSeen: false,
    }
  },

  token(stream, state) {
    // ── multi-line /* … */ block comment ──────────────────────────────
    if (state.inBlockComment) {
      while (!stream.eol()) {
        if (stream.match('*/')) {
          state.inBlockComment = false
          return 'comment'
        }
        stream.next()
      }
      return 'comment'
    }

    // ── multi-line ``` fence ──────────────────────────────────────────
    if (state.inFence) {
      if (stream.sol() && stream.match(/```\s*$/)) {
        state.inFence = false
        return 'meta'
      }
      stream.skipToEnd()
      return 'comment'
    }

    // ── nested contexts win over line mode ────────────────────────────
    const ctx = state.ctxStack[state.ctxStack.length - 1]
    if (ctx === 'directive') return directiveToken(stream, state)
    if (ctx === 'interp') return interpToken(stream, state)

    // ── start-of-line dispatch ────────────────────────────────────────
    if (stream.sol()) {
      state.mode = 'prose'
      // `<…>` / `{…}` always close on their own line, so any leftover
      // context is a defensive reset (never a real multi-line span).
      state.ctxStack = []
      state.verbSeen = false
      state.headSeen = false
      stream.eatSpace()
      const opener = lineStart(stream, state)
      if (opener !== null) return opener
      // fall through to inline tokenising in the mode just set
    }

    return inlineToken(stream, state)
  },

  languageData: {
    commentTokens: { line: '//', block: { open: '/*', close: '*/' } },
    indentOnInput: /^\s*[*+-]\s/,
  },
}

/**
 * Classify the start of a line. Consumes the opener token (returning its
 * style) and leaves `state.mode` set for the remainder of the line, or
 * returns `null` to hand a mode-less line straight to `inlineToken`.
 */
function lineStart(stream: StringStream, state: LoomState): string | null {
  const rest: string = stream.string.slice(stream.pos)
  if (rest.length === 0) return null

  // Whole-line // comment.
  if (stream.match(/^\/\/.*$/)) return 'comment'

  // Block-comment opener.
  if (stream.match('/*')) {
    state.inBlockComment = true
    return 'comment'
  }

  // ```fence opener (optional tag).
  if (stream.match(/^```[A-Za-z0-9_-]*\s*$/)) {
    state.inFence = true
    return 'meta'
  }

  // # Title  (not ##)
  if (stream.match(/^#(?!#)/)) {
    stream.skipToEnd()
    return 'heading'
  }

  // == knot marker → name is a jump label. A knot opens a beat: its body
  // is dialogue/prose, so leave hook/opener territory.
  if (stream.match(/^==/)) {
    state.mode = 'knot'
    state.block = 'beat'
    return 'kw'
  }

  // Scene heading (INT./EXT./…) — whole line.
  if (SCENE_HEADING_RE.test(rest)) {
    stream.skipToEnd()
    return 'heading'
  }

  // let name = expr — only a real binding (must have `=`), never prose
  // like "let me think" (mirrors the lexer's letBinding rule).
  if (/^let\s+[A-Za-z_][A-Za-z0-9_]*\s*=/.test(rest)) {
    stream.match(/^let(?=\s)/)
    state.mode = 'expr'
    return 'kw'
  }

  // -> divert  /  <- tunnel return
  if (stream.match(/^->/)) {
    state.mode = 'divert'
    return 'kw'
  }
  if (stream.match(/^<-\s*$/)) return 'kw'

  // * / + choice marker (requires trailing space).
  if (stream.match(/^[*+](?=\s)/)) {
    state.mode = 'prose'
    return 'kw'
  }

  // Declaration opener — `KEYWORD Name [is X, Y]` — opens a declaration
  // body where hooks / body-opener keywords are live.
  if (DECLARATION_RE.test(rest)) {
    stream.match(DECLARATION_RE)
    state.mode = 'decl'
    state.block = 'decl'
    return 'kw'
  }

  // Multi-word field / contract key (`active when:`, `completes when:`,
  // `with topic:`). Requires a trailing colon, so it can't trip prose.
  if (MULTIWORD_KEY_RE.test(rest)) {
    stream.match(MULTIWORD_KEY_RE)
    state.mode = 'expr'
    return 'kw'
  }

  // `on <event>` hook line — ONLY inside a declaration body. In a beat's
  // dialogue a lowercase-leading "on the table…" is plain prose.
  if (state.block === 'decl' && stream.match(/^on(?=\s)/)) {
    state.mode = 'hook'
    return 'kw'
  }

  // Property / contract / field key — `key: …`.
  const prop = PROPERTY_RE.exec(rest)
  if (prop) {
    const key = prop[1]
    stream.match(IDENT_RE)
    stream.match(/^\s*:/)
    // A value that opens like a typed slot / number / expression is
    // tokenised as code; free-form prose values (titles, labels) are not,
    // so `title: This is fine` never highlights `is`.
    const value = stream.string.slice(stream.pos).trim()
    const typed = valueLooksTyped(value) || FIELD_EXPR_KEYS.has(key)
    state.mode = typed ? 'value' : 'prose'
    return CONTRACT_SET.has(key) ? 'kw' : 'prop'
  }

  // Body-opener keyword (`stat …`, `goal …`, `reacts … -> tag`, `yield …`)
  // — only inside a declaration body, so lowercase-leading dialogue like
  // "wait here." / "run!" in a beat stays prose. A `beat …` / `fill …`
  // opener itself introduces a prose body, so it flips into beat mode.
  if (state.block === 'decl') {
    const opener = stream.match(BODY_OPENER_RE) as RegExpMatchArray | null
    if (opener) {
      if (opener[0] === 'beat' || opener[0] === 'fill') state.block = 'beat'
      state.mode = 'expr'
      return 'kw'
    }
  }

  // Whole-line parenthetical (performer cue) — but only when the parens
  // wrap the whole line with no nested `)` (improv blocks fall through).
  if (stream.match(/^\([^)]*\)\s*$/)) return 'comment'

  // ALL-CAPS speaker cue.
  if (SPEAKER_RE.test(rest)) {
    stream.skipToEnd()
    return 'speaker'
  }

  // Anything else is prose/dialogue.
  state.mode = 'prose'
  return null
}

/** Does a property value open like a typed slot / numeric / expression?
 *  Conservative on purpose: a free-form prose value (a title, a label)
 *  must read as prose so a stray `is` / `of` never lights up. */
function valueLooksTyped(v: string): boolean {
  if (v.length === 0) return false
  if (/^-?\d/.test(v)) return true // numbers, ranges, defaults
  if (/^(any|list|map|range|bool|int|float|text|true|false|nil)\b/.test(v)) return true
  if (v.includes('|')) return true // sum type `a | b | c`
  return false
}

/** Tokenise the remainder of a line according to `state.mode`. */
function inlineToken(stream: StringStream, state: LoomState): string | null {
  if (stream.eatSpace()) return null

  // Inline comments (whitespace-bounded) apply in every mode.
  if (commentBoundaryOk(stream)) {
    if (stream.match(/^\/\/.*$/)) return 'comment'
    if (stream.match('/*')) {
      state.inBlockComment = true
      return 'comment'
    }
  }

  // Enter a `{ … }` interpolation — only when it closes on the same line,
  // so a stray `{` in prose never leaks the context onto the next line.
  if (peekInterp(stream)) {
    stream.next() // consume '{'
    state.ctxStack.push('interp')
    return 'interp'
  }
  // Enter a `< … >` directive when it looks like one (`<word …>` on the
  // line) — never on a bare `a < b` comparison in prose.
  if (peekDirective(stream)) {
    // A whole-line directive (nothing before `<`, line ends with `>`) uses
    // its LAST `>` as the closer, so a `>=` / `>` comparison inside stays
    // in the expression; an inline directive followed by prose closes at
    // the first `>`.
    state.dirLastCloser =
      stream.string.slice(0, stream.pos).trim().length === 0 &&
      stream.string.trimEnd().endsWith('>')
    stream.next() // consume '<'
    state.ctxStack.push('directive')
    state.verbSeen = false
    return 'meta'
  }
  // [suppressed] tail.
  if (stream.match(/^\[[^\]\n]*\]/)) return 'comment'

  switch (state.mode) {
    case 'decl':
      return declToken(stream, state)
    case 'knot':
      return knotToken(stream, state)
    case 'divert':
      return divertToken(stream, state)
    case 'hook':
      return hookToken(stream, state)
    case 'value':
      return exprToken(stream, state, /* full */ false)
    case 'expr':
      return exprToken(stream, state, /* full */ true)
    case 'prose':
    default:
      return proseToken(stream)
  }
}

/** True when `{` at the cursor opens an interpolation that closes on the
 *  same line (interpolations are single-line — spec §12). */
function peekInterp(stream: StreamLike): boolean {
  if (stream.string.charAt(stream.pos) !== '{') return false
  return stream.string.indexOf('}', stream.pos + 1) !== -1
}

/** True when `<` at the cursor opens a directive rather than a `<` glyph. */
function peekDirective(stream: StreamLike): boolean {
  const s = stream.string
  if (s.charAt(stream.pos) !== '<') return false
  const next = s.charAt(stream.pos + 1)
  if (!/[A-Za-z_]/.test(next)) return false
  return s.indexOf('>', stream.pos + 1) !== -1
}

/** Declaration line after the kind keyword: `Name [is Mixin, …]`. */
function declToken(stream: StringStream, state: LoomState): string | null {
  if (!state.headSeen) {
    if (stream.match(IDENT_RE)) {
      state.headSeen = true
      return 'type'
    }
  } else {
    if (stream.match(/^is(?=\s)/)) return 'kw'
    if (stream.match(IDENT_RE)) return 'type'
  }
  if (stream.match(/^[(),]/)) return 'op'
  stream.next()
  return null
}

/** Knot line after `==`: name is a jump label. */
function knotToken(stream: StringStream, state: LoomState): string | null {
  if (!state.headSeen && stream.match(IDENT_RE)) {
    state.headSeen = true
    return 'label'
  }
  if (stream.match(/^[(),]/)) return 'op'
  if (stream.match(IDENT_RE)) return null
  stream.next()
  return null
}

/** Divert line after `->`: the target is a jump label, then any
 *  `with k: v` tail is an expression. */
function divertToken(stream: StringStream, state: LoomState): string | null {
  if (!state.headSeen) {
    if (stream.match(/^END\b/)) {
      state.headSeen = true
      state.mode = 'expr'
      return 'kw'
    }
    if (stream.match(/^[A-Za-z_][A-Za-z0-9_/#.]*/)) {
      state.headSeen = true
      state.mode = 'expr'
      return 'label'
    }
  }
  return exprToken(stream, state, true)
}

/** `on <event>` trigger: live/structural words are keywords, the event
 *  name + qualified targets are labels, then a `-> beat` / `: none` tail. */
function hookToken(stream: StringStream, state: LoomState): string | null {
  if (stream.match(/^->/)) {
    state.mode = 'divert'
    state.headSeen = false
    return 'kw'
  }
  if (stream.match(NUMBER_RE)) return 'num'
  if (stream.match(/^[:]/)) return 'op'
  const word = stream.match(IDENT_RE) as RegExpMatchArray | null
  if (word) {
    return HOOK_WORDS.has(word[0]) ? 'kw' : 'label'
  }
  if (stream.match(/^[.]/)) return 'op'
  stream.next()
  return null
}

/**
 * Expression tokeniser. `full` = an expression context (directive args,
 * interpolation, `let` RHS, hook/meridian bodies) where reserved / live /
 * simulacra / meridian keywords are highlighted; otherwise a property
 * value, where only slot-type words + literals are.
 */
function exprToken(stream: StringStream, state: LoomState, full: boolean): string | null {
  if (stream.match(/^->/)) {
    state.mode = 'divert'
    state.headSeen = false
    return 'kw'
  }
  if (stream.match(/^"(?:\\.|[^"\\])*"/)) return 'str'
  if (stream.match(NUMBER_RE)) return 'num'
  if (stream.match(/^\b(?:true|false|nil)\b/)) return 'atom'
  if (stream.match(full ? EXPR_KEYWORD_RE : VALUE_KEYWORD_RE)) return 'kw'
  if (stream.match(IDENT_RE)) return null
  if (stream.match(OPERATOR_RE)) return 'op'
  stream.next()
  return null
}

/** Prose / dialogue: no bare keywords, numbers or operators — only the
 *  embedded `{…}` / `<…>` / `[…]` / comments handled by `inlineToken`. A
 *  mid-line `->` stays prose: Loom has no inline diverts (the parser stores
 *  action/choice text verbatim — a divert is its own line). */
function proseToken(stream: StringStream): string | null {
  const s: string = stream.string
  let i: number = stream.pos
  // Advance to the next character that could start a special token.
  while (i < s.length && !'{<[/'.includes(s.charAt(i))) i += 1
  if (i === stream.pos) stream.next()
  else stream.pos = i
  return null
}

/** Is the cursor sitting on this directive's closing `>`? For a whole-line
 *  directive that is the LAST `>` on the line, so a `>=` / `>` comparison
 *  in the condition stays part of the expression. */
function atDirectiveClose(stream: StreamLike, state: LoomState): boolean {
  if (stream.string.charAt(stream.pos) !== '>') return false
  if (!state.dirLastCloser) return true
  return stream.string.indexOf('>', stream.pos + 1) === -1
}

/** Inside a `< … >` directive: verb first, then expression args. */
function directiveToken(stream: StringStream, state: LoomState): string | null {
  if (atDirectiveClose(stream, state)) {
    stream.next()
    state.ctxStack.pop()
    return 'meta'
  }
  if (stream.eatSpace()) return null
  if (!state.verbSeen) {
    if (stream.match(SYNTACTIC_RE)) {
      state.verbSeen = true
      return 'kw'
    }
    // Builtin verbs (`set`, `broadcast`, `cast`) and user directives
    // (`reveal`, `capture`) both read as function-like → blue.
    if (stream.match(IDENT_RE)) {
      state.verbSeen = true
      return 'fn'
    }
  }
  // Nested interpolation inside a directive (`<respond: {a}, {b}>`).
  if (peekInterp(stream)) {
    stream.next()
    state.ctxStack.push('interp')
    return 'interp'
  }
  return exprInner(stream)
}

/** Inside a `{ … }` interpolation: pure expression. Popping restores the
 *  parent context — e.g. back into the enclosing `< … >` directive. */
function interpToken(stream: StringStream, state: LoomState): string | null {
  if (stream.match('}')) {
    state.ctxStack.pop()
    return 'interp'
  }
  return exprInner(stream)
}

/** One expression token inside a `< >` / `{ }` context. Never consumes the
 *  closer — the ctx handler does, on the next call. */
function exprInner(stream: StringStream): string | null {
  if (stream.eatSpace()) return null
  if (stream.match(/^"(?:\\.|[^"\\])*"/)) return 'str'
  if (stream.match(NUMBER_RE)) return 'num'
  if (stream.match(/^\b(?:true|false|nil)\b/)) return 'atom'
  if (stream.match(EXPR_KEYWORD_RE)) return 'kw'
  if (stream.match(IDENT_RE)) return null
  if (stream.match(OPERATOR_RE)) return 'op'
  stream.next()
  return null
}

export function loomLanguage(): LanguageSupport {
  return new LanguageSupport(StreamLanguage.define(parser))
}

/** The raw `StreamParser` — exported for unit tests (see
 *  `loom-language.test.ts`); `loomLanguage()` is the app entry point. */
export const loomStreamParser: StreamParser<LoomState> = parser
