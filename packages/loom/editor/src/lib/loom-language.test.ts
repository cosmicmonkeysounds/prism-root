import { describe, expect, it } from 'vitest'
import { StringStream } from '@codemirror/language'
import { loomStreamParser as P } from './loom-language'

/** Tokenise a whole source document, carrying parser state across lines,
 *  into `{ text, style }` tokens per line. Mirrors CodeMirror's own
 *  `readToken` loop (advance-or-throw). */
function tokenize(source: string): { text: string; style: string | null }[][] {
  const state = P.startState!(2)
  return source.split('\n').map((line) => {
    const stream = new StringStream(line, 4, 2, 0)
    const out: { text: string; style: string | null }[] = []
    let guard = 0
    while (!stream.eol()) {
      stream.start = stream.pos
      const style = P.token(stream, state) ?? null
      if (stream.pos <= stream.start) stream.pos = stream.string.length // safety
      out.push({ text: stream.current(), style })
      if (++guard > 500) throw new Error(`tokenizer stuck on: ${line}`)
    }
    return out
  })
}

/** Find the style applied to the token whose text is `needle` (a trailing
 *  `:` on a property/contract key is ignored, since the key + colon tokenise
 *  together). */
function styleOf(line: { text: string; style: string | null }[], needle: string): string | null {
  const tok = line.find((t) => t.text.trim().replace(/:$/, '') === needle)
  return tok ? tok.style : '<absent>'
}

/** All styles touching any token that contains `needle` as a word. */
function stylesFor(
  line: { text: string; style: string | null }[],
  needle: string,
): (string | null)[] {
  return line.filter((t) => t.text.includes(needle)).map((t) => t.style)
}

describe('loom-language tokenizer', () => {
  it('does NOT highlight reserved words in prose/dialogue (the screenshot bug)', () => {
    const [line] = tokenize('    Tonight the room is the network and you are')
    // `is`, `and`, `are` must all be plain prose (null), never `kw`.
    expect(styleOf(line, 'is')).toBe('<absent>') // folded into a prose run
    expect(line.every((t) => t.style !== 'kw')).toBe(true)
    // The whole line is one (or few) prose runs with no keyword styling.
    expect(line.flatMap((t) => (t.style ? [t.style] : [])).filter((s) => s !== 'comment')).toEqual(
      [],
    )
  })

  it('does not highlight `is`/`with` inside a free-form title value', () => {
    const [line] = tokenize('title: This is a story with heart')
    expect(styleOf(line, 'title')).toBe('kw') // contract key
    expect(line.filter((t) => t.style === 'kw').length).toBe(1) // only the key
  })

  it('highlights ROLE declaration keyword and name', () => {
    const [line] = tokenize('ROLE Guest')
    expect(styleOf(line, 'ROLE')).toBe('kw')
    expect(styleOf(line, 'Guest')).toBe('type')
  })

  it('highlights every declaration kind + name + is-mixin', () => {
    for (const [src, kw, name] of [
      ['CHARACTER Wren is Keeper', 'CHARACTER', 'Wren'],
      ['FACTION Mods', 'FACTION', 'Mods'],
      ['SPACE Forums', 'SPACE', 'Forums'],
      ['CHANNEL announcements', 'CHANNEL', 'announcements'],
      ['PERSON jamie_lee', 'PERSON', 'jamie_lee'],
      ['ROSTER preview_night', 'ROSTER', 'preview_night'],
      ['TRAIT Scanner', 'TRAIT', 'Scanner'],
    ] as const) {
      const [line] = tokenize(src)
      expect(styleOf(line, kw), `${src}: keyword`).toBe('kw')
      expect(styleOf(line, name), `${src}: name`).toBe('type')
    }
    const [mix] = tokenize('CHARACTER Wren is Keeper')
    expect(styleOf(mix, 'is')).toBe('kw')
    expect(styleOf(mix, 'Keeper')).toBe('type')
  })

  it('highlights the knot marker AND its name', () => {
    const [line] = tokenize('== doors_open')
    expect(styleOf(line, '==')).toBe('kw')
    expect(styleOf(line, 'doors_open')).toBe('label')
  })

  it('highlights on-hooks: keyword + event, incl. live verbs and numbers', () => {
    const cases: [string, [string, string | null][]][] = [
      ['  on captured', [['on', 'kw'], ['captured', 'label']]],
      ['  on escape', [['on', 'kw'], ['escape', 'label']]],
      ['  on enters Internet', [['on', 'kw'], ['enters', 'kw'], ['Internet', 'label']]],
      ['  on scan other', [['on', 'kw'], ['scan', 'label'], ['other', 'label']]],
      ['  on trust passes 80', [['on', 'kw'], ['passes', 'kw'], ['80', 'num']]],
    ]
    for (const [src, checks] of cases) {
      // Hooks are only live inside a declaration body.
      const line = tokenize(`CHARACTER Guest\n${src}`)[1]
      for (const [word, style] of checks) {
        expect(styleOf(line, word), `${src}: ${word}`).toBe(style)
      }
    }
  })

  it('highlights directives: syntactic vs builtin/user verbs + expr args', () => {
    const [ifLine] = tokenize('    <if: self.heat >= 75>')
    expect(styleOf(ifLine, 'if')).toBe('kw') // syntactic → violet
    expect(styleOf(ifLine, 'self')).toBe('kw') // reserved inside expr

    const [bc] = tokenize('    <broadcast: freedom to participant(self)>')
    expect(styleOf(bc, 'broadcast')).toBe('fn') // builtin → blue function
    expect(styleOf(bc, 'to')).toBe('kw')
    expect(styleOf(bc, 'participant')).toBe('kw')

    const [set] = tokenize('    <set: self.score -= 25>')
    expect(styleOf(set, 'set')).toBe('fn')
    expect(styleOf(set, '25')).toBe('num')

    const [user] = tokenize('    <reveal: Glitchers>')
    expect(styleOf(user, 'reveal')).toBe('fn') // user directive → blue
  })

  it('treats `>=` inside a directive as an operator, not the closer', () => {
    const [line] = tokenize('    <if: self.heat >= 75>')
    expect(styleOf(line, '75')).toBe('num') // not stranded outside the directive
    // exactly two `meta` tokens: the opening `<` and the final `>`.
    expect(line.filter((t) => t.style === 'meta').length).toBe(2)
    expect(line.filter((t) => t.text === '>').every((t) => t.style === 'op' || t.style === 'meta')).toBe(true)
  })

  it('highlights typed-slot property values (any of X, ranges, bool)', () => {
    const [f] = tokenize('  faction: any of FACTION')
    expect(styleOf(f, 'faction')).toBe('prop')
    expect(styleOf(f, 'any')).toBe('kw')
    expect(styleOf(f, 'of')).toBe('kw')

    const [s] = tokenize('  score: 0 to 1000 = 0')
    expect(stylesFor(s, '0').every((x) => x === 'num')).toBe(true)
    expect(styleOf(s, 'to')).toBe('kw')

    const [b] = tokenize('  captured: bool = false')
    expect(styleOf(b, 'bool')).toBe('kw')
    expect(styleOf(b, 'false')).toBe('atom')
  })

  it('highlights standalone diverts + targets', () => {
    const [d] = tokenize('-> doors_open')
    expect(styleOf(d, '->')).toBe('kw')
    expect(styleOf(d, 'doors_open')).toBe('label')

    const [end] = tokenize('-> END')
    expect(styleOf(end, 'END')).toBe('kw')
  })

  it('does NOT treat a mid-prose `->` as a divert (Loom has no inline diverts)', () => {
    const [choice] = tokenize('* Ring the bell -> ringing')
    expect(styleOf(choice, '*')).toBe('kw') // only the choice marker
    expect(choice.filter((t) => t.style === 'kw').length).toBe(1)
    expect(choice.every((t) => t.style !== 'label')).toBe(true) // `ringing` stays prose
    const [prose] = tokenize('    I turned -> and walked away.')
    expect(prose.every((t) => t.style === null)).toBe(true)
  })

  it('does NOT highlight hook/opener/let words in a beat’s dialogue (block context)', () => {
    // Under a `== knot` beat, indented lines are dialogue — lowercase-leading
    // `on` / `wait` / `run` / `let` must stay plain prose, not keywords.
    const doc = [
      '== scene',
      '  NARRATOR',
      '    on the table there was a letter.',
      '    wait here for a moment.',
      '    run before they catch you.',
      '    let me think about that.',
      '    every night the same dream.',
    ].join('\n')
    const lines = tokenize(doc)
    for (const li of [2, 3, 4, 5, 6]) {
      expect(lines[li].every((tk) => tk.style === null), `line ${li + 1} all prose`).toBe(true)
    }
  })

  it('DOES highlight hooks/openers inside a declaration body (block context)', () => {
    const doc = ['CHARACTER Wren', '  on captured', '  stat hp = 10'].join('\n')
    const lines = tokenize(doc)
    expect(styleOf(lines[1], 'on')).toBe('kw')
    expect(styleOf(lines[1], 'captured')).toBe('label')
    expect(styleOf(lines[2], 'stat')).toBe('kw')
  })

  it('only treats `let` as a binding when it has `=`', () => {
    expect(styleOf(tokenize('let count = 0')[0], 'let')).toBe('kw')
    expect(tokenize('    let me think.')[0].every((t) => t.style === null)).toBe(true)
  })

  it('highlights multi-word field keys (active when:, completes when:)', () => {
    const doc = ['CHARACTER W', '  goal g', '    active when: true', '    completes when: W.knows.x'].join(
      '\n',
    )
    const lines = tokenize(doc)
    expect(lines[2][0].style).toBe('kw') // `active when:`
    expect(styleOf(lines[2], 'true')).toBe('atom')
    expect(lines[3][0].style).toBe('kw') // `completes when:`
  })

  it('keeps the directive context across a nested interpolation (ctx stack)', () => {
    const [line] = tokenize('    <respond: score {a}, trust {b}>')
    expect(styleOf(line, 'respond')).toBe('fn')
    // After the first {a} closes we must still be inside the directive, so
    // `trust` is an expression token and the FINAL `>` is the closer (meta).
    expect(line.filter((t) => t.style === 'meta').length).toBe(2) // opening `<` + closing `>`
    expect(line[line.length - 1].text).toBe('>')
    expect(line[line.length - 1].style).toBe('meta')
  })

  it('classifies headers, speakers, comments, fences', () => {
    const header = tokenize('# Escape the Internet')[0]
    expect(header[0].style).toBe('heading') // whole line is one heading token
    expect(header.map((t) => t.text).join('')).toContain('Internet')
    expect(tokenize('# Title')[0][0].style).toBe('heading')
    expect(styleOf(tokenize('NARRATOR')[0], 'NARRATOR')).toBe('speaker')
    expect(tokenize('WREN | FISHER')[0][0].style).toBe('speaker') // one whole-line cue
    expect(tokenize('// a comment')[0][0].style).toBe('comment')
    const fence = tokenize('```note\nstage note\n```')
    expect(fence[0][0].style).toBe('meta') // opener
    expect(fence[1][0].style).toBe('comment') // content
    expect(fence[2][0].style).toBe('meta') // closer
  })

  it('highlights interpolation inside prose', () => {
    const [line] = tokenize('    Score {guest.score}. Talk.')
    // The braces carry the interp style; `Score`/`Talk.` stay prose.
    expect(line.some((t) => t.style === 'interp')).toBe(true)
    expect(line.some((t) => t.text.includes('Score') && t.style === null)).toBe(true)
  })

  it('highlights STATS body openers (stat/attribute/axis/pool)', () => {
    const lines = tokenize(
      ['STATS Combat', '  attribute strength = 10, range 1 to 30', '  axis level', '  stat max_health = 50 + strength * 5'].join(
        '\n',
      ),
    )
    expect(styleOf(lines[1], 'attribute')).toBe('kw')
    expect(styleOf(lines[1], 'to')).toBe('kw')
    expect(styleOf(lines[2], 'axis')).toBe('kw')
    expect(styleOf(lines[3], 'stat')).toBe('kw')
  })
})
