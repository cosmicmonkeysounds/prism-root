// Word blocks — the flattened, display-ready projection of a beat's
// body for the EXPANDED project-view card. Every body item becomes one
// typed block (prose, dialogue, directive, choice, branch head, divert,
// slot hole), nested control flow carried as an indent depth, so the
// whole beat reads top-to-bottom inside the card without drilling in.
//
// Each block also anchors back into authored source: `topIndex` names
// the top-level body item it belongs to (insert/delete ops), and
// `spanStart`/`spanEnd` are the exact offsets an inline edit rewrites —
// only ever the lines whose text the block itself displays, so a
// parent block and its child blocks never own overlapping edit
// surfaces. Branch heads and slot holes are display-only (null spans).
//
// Kept out of the component files (pure data + sizing) so the flow
// builder, the ELK size estimate, the renderer, and the edit wiring
// share one source.

import { divertSpan, type BodyItem, type Span } from '@loom/core/parser'
import { BODY_BLOCK_CHARS_PER_LINE, BODY_BLOCK_LINE_H } from './metrics'

/** Const-object enum (`erasableSyntaxOnly`-safe) of block kinds. */
export const WordBlockKind = {
  Prose: 'prose',
  Dialogue: 'dialogue',
  Directive: 'directive',
  Choice: 'choice',
  Branch: 'branch',
  Divert: 'divert',
  Slot: 'slot',
} as const

export type WordBlockKind = (typeof WordBlockKind)[keyof typeof WordBlockKind]

export interface WordBlock {
  kind: WordBlockKind
  /** Nesting depth (choice bodies, `<if:>` arms, …) → indent. */
  depth: number
  /** Leading label — speaker, branch-arm tag, `*`/`+` marker. */
  label: string | null
  text: string
  /** Index of the top-level body item this block belongs to. */
  topIndex: number
  /** Exact source offsets an inline edit rewrites (null → display-only). */
  spanStart: number | null
  spanEnd: number | null
  /**
   * Edit only the first physical line of the span — choice markers and
   * bare dialogue cues own their opening line; their bodies are the
   * children's own blocks.
   */
  firstLineOnly: boolean
}

type BlockEdit = { span: Span; firstLineOnly?: boolean }

function block(
  kind: WordBlockKind,
  depth: number,
  label: string | null,
  text: string,
  topIndex: number,
  edit: BlockEdit | null = null,
): WordBlock {
  return {
    kind,
    depth,
    label,
    text,
    topIndex,
    spanStart: edit !== null ? edit.span.start.offset : null,
    spanEnd: edit !== null ? edit.span.end.offset : null,
    firstLineOnly: edit?.firstLineOnly === true,
  }
}

/** Flatten one lowered body list into display blocks. */
export function toWordBlocks(items: BodyItem[]): WordBlock[] {
  return lower(items, 0, null)
}

function lower(items: BodyItem[], depth: number, ancestorTop: number | null): WordBlock[] {
  const out: WordBlock[] = []
  items.forEach((item, i) => {
    const top = ancestorTop ?? i
    const walkArm = (label: string, body: BodyItem[]): void => {
      if (body.length === 0) return
      out.push(block(WordBlockKind.Branch, depth, label, '', top))
      out.push(...lower(body, depth + 1, top))
    }
    switch (item.kind) {
      case 'action':
      case 'sceneHeading':
        out.push(block(WordBlockKind.Prose, depth, null, item.value.value, top, { span: item.value.span }))
        break
      case 'metadata':
        out.push(block(WordBlockKind.Directive, depth, '```', item.value.value, top, { span: item.value.span }))
        break
      case 'inlineLet':
        out.push(
          block(
            WordBlockKind.Directive,
            depth,
            null,
            `<let: ${item.value.name} = ${item.value.expression}>`,
            top,
            { span: item.value.span },
          ),
        )
        break
      case 'directive':
        out.push(block(WordBlockKind.Directive, depth, null, item.value.raw, top, { span: item.value.span }))
        break
      case 'directiveBlock':
        // The header line only — the body items are their own blocks.
        out.push(
          block(WordBlockKind.Directive, depth, null, item.value.directive.raw, top, {
            span: item.value.directive.span,
          }),
        )
        out.push(...lower(item.value.body, depth + 1, top))
        break
      case 'dialogue': {
        const speaker =
          item.value.speakers.length > 1 ? item.value.speakers.join(' | ') : item.value.speaker
        // Leading prose lines render inside the dialogue block; nested
        // control flow falls through below it (mirrors the drill-in).
        const lines: string[] = []
        const rest: BodyItem[] = []
        let lastMerged: Span | null = null
        for (const child of item.value.body) {
          if (child.kind === 'action' && rest.length === 0) {
            lines.push(child.value.value)
            lastMerged = child.value.span
          } else rest.push(child)
        }
        // Edit surface: the cue line plus exactly the merged prose. A
        // bare cue over non-prose children owns its first line only.
        const edit: BlockEdit =
          lastMerged !== null
            ? { span: { start: item.value.span.start, end: lastMerged.end } }
            : { span: item.value.span, firstLineOnly: true }
        out.push(block(WordBlockKind.Dialogue, depth, speaker, lines.join('\n'), top, edit))
        out.push(...lower(rest, depth + 1, top))
        break
      }
      case 'choice':
        out.push(
          block(WordBlockKind.Choice, depth, item.value.sticky ? '+' : '*', item.value.text, top, {
            span: item.value.span,
            firstLineOnly: true,
          }),
        )
        out.push(...lower(item.value.body, depth + 1, top))
        break
      case 'conditional':
        for (const arm of item.value.arms) {
          walkArm(arm.condition === null ? '<else>' : `<if: ${arm.condition}>`, arm.body)
        }
        break
      case 'match':
        out.push(block(WordBlockKind.Branch, depth, `<match: ${item.value.scrutinee}>`, '', top))
        for (const arm of item.value.arms) walkArm(arm.pattern, arm.body)
        break
      case 'eachVisit':
        walkArm('first', item.value.first)
        walkArm('then', item.value.then)
        walkArm('finally', item.value.finally)
        break
      case 'afterMorph':
        walkArm(`<after: ${item.value.condition}>`, item.value.after)
        walkArm('<otherwise>', item.value.otherwise)
        break
      case 'divert': {
        const d = item.value
        const text =
          d.kind === 'end' ? '-> END'
          : d.kind === 'return' ? '<- return'
          : d.kind === 'tunnel' ? `(${displayTarget(d)}) ->`
          : `-> ${displayTarget(d)}`
        out.push(block(WordBlockKind.Divert, depth, null, text, top, { span: divertSpan(d) }))
        break
      }
      case 'slotPlaceholder':
        out.push(block(WordBlockKind.Slot, depth, 'slot', item.value.name, top))
        break
      default:
        break
    }
  })
  return out
}

function displayTarget(d: Extract<BodyItem, { kind: 'divert' }>['value']): string {
  if (d.kind !== 'to' && d.kind !== 'tunnel') return '?'
  const t = d.target
  const base = t.qualifier === null ? t.name : `${t.qualifier}.${t.name}`
  return t.knot === null ? base : `${base}#${t.knot}`
}

/**
 * The exact `[start, end)` source range `block` edits — trimmed to the
 * first physical line when the block only owns its opening line.
 * `null` for display-only blocks or a stale span.
 */
export function blockEditRange(source: string, block: WordBlock): [number, number] | null {
  if (block.spanStart === null || block.spanEnd === null) return null
  const start = block.spanStart
  let end = Math.min(block.spanEnd, source.length)
  if (block.firstLineOnly) {
    const nl = source.indexOf('\n', start)
    if (nl !== -1 && nl < end) end = nl
  }
  return start < 0 || start > end ? null : [start, end]
}

/**
 * Estimated pixel height of a rendered block list (ELK's first-pass
 * seed; the measured second pass corrects any drift).
 */
export function wordBlocksHeight(blocks: WordBlock[]): number {
  let h = 0
  for (const b of blocks) {
    const chars = b.text.length + (b.label?.length ?? 0)
    const lines = Math.max(1, Math.ceil(chars / BODY_BLOCK_CHARS_PER_LINE))
    h += 8 + lines * BODY_BLOCK_LINE_H
  }
  return h
}
