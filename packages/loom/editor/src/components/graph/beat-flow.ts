// Beat body → drill-in flow, 1:1 with the language.
//
// Walks a lowered `BodyItem[]` into a top-down chain: prose / dialogue /
// directives link linearly, consecutive `*`/`+` choices fan out (their
// option bodies fall through to the item AFTER the menu, mirroring the
// sim's continuation semantics), `<if:>`/`<match:>`/`<each visit>`/
// `<after:>` become branch heads with labeled arm chains that re-merge,
// and diverts terminate a chain in an exit node. The walker returns
// "open tails" — nodes whose flow continues — exactly like the executor
// treats fall-through.

import type { Edge, Node } from '@xyflow/react'
import { MarkerType } from '@xyflow/react'
import type { BodyItem, DivertTarget } from '@loom/core/parser'
import type { GraphBeat, StoryGraph } from '@loom/core/lsp'
import type { LoomSpan } from '@/lib/loom-ast'
import type { SourceAnchor } from './body-nodes'
import {
  BRANCH_H,
  BRANCH_W,
  EXIT_H,
  EXIT_W,
  START_W,
  bodyChoiceSize,
  bodyDialogueSize,
  bodyTextSize,
} from './metrics'
import type { LayoutEdgeIn, LayoutNodeIn } from './layout'

export interface BeatFlow {
  nodes: Node[]
  edges: Edge[]
  layoutNodes: LayoutNodeIn[]
  layoutEdges: LayoutEdgeIn[]
}

interface Builder {
  nodes: Node[]
  edges: Edge[]
  layoutNodes: LayoutNodeIn[]
  layoutEdges: LayoutEdgeIn[]
  seq: number
  /** Anchors are only exact for `structural: "file"` beats. */
  anchored: boolean
  uri: string | null
  graph: StoryGraph
  owner: string | null
  castFallback: string | null
}

const displayTarget = (t: DivertTarget): string => {
  const base = t.qualifier === null ? t.name : `${t.qualifier}.${t.name}`
  return t.knot === null ? base : `${base}#${t.knot}`
}

/** Mirror of the core's static resolution (owner, then cast fallback, then flat). */
function resolveKey(b: Builder, t: DivertTarget): string | null {
  if (t.qualifier !== null) {
    const q = t.qualifier === 'self' || t.qualifier === 'me' ? (b.owner ?? b.castFallback) : t.qualifier
    if (q !== null && b.graph.beats.has(`${q}.${t.name}`)) return `${q}.${t.name}`
  }
  return b.graph.beats.has(t.name) ? t.name : null
}

function anchorOf(b: Builder, span: LoomSpan | undefined): SourceAnchor {
  if (!b.anchored || b.uri === null || span === undefined) return null
  return { uri: b.uri, span }
}

function addNode(
  b: Builder,
  type: string,
  data: Record<string, unknown>,
  size: { width: number; height: number },
): string {
  const id = `b${b.seq++}`
  b.nodes.push({ id, type, position: { x: 0, y: 0 }, data })
  b.layoutNodes.push({ id, width: size.width, height: size.height })
  return id
}

function connect(b: Builder, tails: string[], to: string, label?: string): void {
  for (const from of tails) {
    const id = `be${b.seq++}`
    b.edges.push({
      id,
      source: from,
      target: to,
      label,
      labelStyle: { fill: '#a1a1aa', fontSize: 9 },
      labelBgStyle: { fill: '#101014', fillOpacity: 0.8 },
      style: { stroke: '#71717a', strokeWidth: 1.3 },
      markerEnd: { type: MarkerType.ArrowClosed, color: '#71717a', width: 13, height: 13 },
    })
    b.layoutEdges.push({ id, source: from, target: to })
  }
}

/** Walk a body list; returns the open tails that continue past it. */
function walkSeq(b: Builder, items: BodyItem[], entry: string[]): string[] {
  let tails = entry
  let i = 0
  while (i < items.length) {
    const item = items[i]!
    if (item.kind === 'choice') {
      const group: Extract<BodyItem, { kind: 'choice' }>[] = []
      while (i < items.length && items[i]!.kind === 'choice') {
        group.push(items[i] as Extract<BodyItem, { kind: 'choice' }>)
        i += 1
      }
      const merged: string[] = []
      for (const c of group) {
        const id = addNode(
          b,
          'bodyChoice',
          {
            text: c.value.text,
            sticky: c.value.sticky,
            suppressed: c.value.suppressed,
            anchor: anchorOf(b, c.value.span),
          },
          bodyChoiceSize(c.value.text),
        )
        connect(b, tails, id)
        merged.push(...walkSeq(b, c.value.body, [id]))
      }
      tails = merged
      continue
    }
    tails = walkItem(b, item, tails)
    i += 1
  }
  return tails
}

function walkItem(b: Builder, item: BodyItem, tails: string[]): string[] {
  switch (item.kind) {
    case 'action':
    case 'sceneHeading': {
      const id = addNode(
        b,
        'bodyText',
        {
          text: item.value.value,
          scene: item.kind === 'sceneHeading',
          anchor: anchorOf(b, item.value.span),
        },
        bodyTextSize(item.value.value),
      )
      connect(b, tails, id)
      return [id]
    }
    case 'metadata': {
      const id = addNode(
        b,
        'bodyText',
        { text: `\`\`\` ${item.value.value}`, anchor: anchorOf(b, item.value.span) },
        bodyTextSize(item.value.value),
      )
      connect(b, tails, id)
      return [id]
    }
    case 'inlineLet': {
      const id = addNode(
        b,
        'bodyDirective',
        { raw: `<let: ${item.value.name} = ${item.value.expression}>`, anchor: anchorOf(b, item.value.span) },
        { width: BRANCH_W + 60, height: BRANCH_H },
      )
      connect(b, tails, id)
      return [id]
    }
    case 'directive': {
      const id = addNode(
        b,
        'bodyDirective',
        { raw: item.value.raw, anchor: anchorOf(b, item.value.span) },
        { width: Math.min(240, 40 + item.value.raw.length * 5.4), height: 28 },
      )
      connect(b, tails, id)
      return [id]
    }
    case 'directiveBlock': {
      const id = addNode(
        b,
        'bodyDirective',
        { raw: item.value.directive.raw, anchor: anchorOf(b, item.value.directive.span) },
        { width: Math.min(240, 40 + item.value.directive.raw.length * 5.4), height: 28 },
      )
      connect(b, tails, id)
      return walkSeq(b, item.value.body, [id])
    }
    case 'slotPlaceholder': {
      const id = addNode(
        b,
        'bodySlot',
        { name: item.value.name, anchor: anchorOf(b, item.value.span) },
        { width: BRANCH_W + 40, height: BRANCH_H },
      )
      connect(b, tails, id)
      return [id]
    }
    case 'dialogue': {
      // Leading prose inside the speaker block renders inside the card;
      // control flow nested in the dialogue chains on after it.
      const lines: string[] = []
      const rest: BodyItem[] = []
      for (const child of item.value.body) {
        if (child.kind === 'action' && rest.length === 0) lines.push(child.value.value)
        else rest.push(child)
      }
      const id = addNode(
        b,
        'bodyDialogue',
        {
          speaker: item.value.speakers.length > 1 ? item.value.speakers.join(' | ') : item.value.speaker,
          lines,
          parenthetical: item.value.parenthetical,
          anchor: anchorOf(b, item.value.span),
        },
        bodyDialogueSize(lines),
      )
      connect(b, tails, id)
      return walkSeq(b, rest, [id])
    }
    case 'conditional': {
      const id = addNode(
        b,
        'bodyBranch',
        { label: '<if:>', anchor: anchorOf(b, item.value.span) },
        { width: BRANCH_W, height: BRANCH_H },
      )
      connect(b, tails, id)
      const out: string[] = []
      let hasElse = false
      for (const arm of item.value.arms) {
        if (arm.condition === null) hasElse = true
        out.push(...walkArm(b, id, arm.condition === null ? 'else' : `if ${arm.condition}`, arm.body))
      }
      if (!hasElse) out.push(id) // no else — the condition may fall through
      return dedupe(out)
    }
    case 'match': {
      const id = addNode(
        b,
        'bodyBranch',
        { label: `<match: ${item.value.scrutinee}>`, anchor: anchorOf(b, item.value.span) },
        { width: Math.max(BRANCH_W, 60 + item.value.scrutinee.length * 5.4), height: BRANCH_H },
      )
      connect(b, tails, id)
      const out: string[] = [id] // no arm matching → falls through silently
      for (const arm of item.value.arms) {
        out.push(...walkArm(b, id, arm.pattern, arm.body))
      }
      return dedupe(out)
    }
    case 'eachVisit': {
      const id = addNode(
        b,
        'bodyBranch',
        { label: '<each visit>', anchor: anchorOf(b, item.value.span) },
        { width: BRANCH_W, height: BRANCH_H },
      )
      connect(b, tails, id)
      const out: string[] = []
      const phases: Array<[string, BodyItem[]]> = [
        ['first', item.value.first],
        ['then', item.value.then],
        ['finally', item.value.finally],
      ]
      for (const [label, body] of phases) out.push(...walkArm(b, id, label, body))
      return dedupe(out)
    }
    case 'afterMorph': {
      const id = addNode(
        b,
        'bodyBranch',
        { label: `<after: ${item.value.condition}>`, anchor: anchorOf(b, item.value.span) },
        { width: Math.max(BRANCH_W, 60 + item.value.condition.length * 5.4), height: BRANCH_H },
      )
      connect(b, tails, id)
      const out: string[] = []
      const arms: Array<[string, BodyItem[]]> = [
        ['after', item.value.after],
        ['otherwise', item.value.otherwise],
      ]
      for (const [label, body] of arms) out.push(...walkArm(b, id, label, body))
      return dedupe(out)
    }
    case 'divert': {
      const d = item.value
      const form =
        d.kind === 'end' ? ('end' as const)
        : d.kind === 'return' ? ('return' as const)
        : d.kind === 'tunnel' ? ('tunnel' as const)
        : ('divert' as const)
      const target = d.kind === 'to' || d.kind === 'tunnel' ? displayTarget(d.target) : null
      const resolved = d.kind === 'to' || d.kind === 'tunnel' ? resolveKey(b, d.target) : null
      const id = addNode(
        b,
        'bodyExit',
        { form, target, resolved, anchor: anchorOf(b, d.span) },
        { width: EXIT_W, height: EXIT_H },
      )
      connect(b, tails, id)
      // A tunnel call RETURNS here and continues; everything else terminates.
      return d.kind === 'tunnel' ? [id] : []
    }
    default:
      return tails
  }
}

/**
 * Walk one labeled arm of a branch head. An empty arm is a pure
 * passthrough (the head itself stays an open tail); a non-empty arm's
 * entry edges (head → first node/option) are retro-labeled with the
 * arm's condition / pattern / phase.
 */
function walkArm(b: Builder, branchId: string, label: string, body: BodyItem[]): string[] {
  if (body.length === 0) return [branchId]
  const before = b.edges.length
  const tails = walkSeq(b, body, [branchId])
  for (let i = before; i < b.edges.length; i += 1) {
    const e = b.edges[i]!
    if (e.source === branchId && e.label === undefined) e.label = label
  }
  return tails.filter((t) => t !== branchId)
}

function dedupe(ids: string[]): string[] {
  return [...new Set(ids)]
}

/** Build the full drill-in flow for one beat. */
export function buildBeatFlow(graph: StoryGraph, beat: GraphBeat, body: BodyItem[]): BeatFlow {
  const b: Builder = {
    nodes: [],
    edges: [],
    layoutNodes: [],
    layoutEdges: [],
    seq: 0,
    anchored: beat.structural === 'file',
    uri: beat.uri,
    graph,
    owner: beat.owner,
    castFallback: beat.cast[0] ?? null,
  }
  const startId = addNode(
    b,
    'bodyStart',
    {
      title: beat.key,
      params: beat.params,
      cast: beat.cast,
      setting: beat.setting,
      anchor: beat.uri !== null && beat.span !== null ? { uri: beat.uri, span: beat.span } : null,
    },
    { width: START_W, height: beat.cast.length > 0 || beat.setting !== null ? 52 : 38 },
  )
  walkSeq(b, body, [startId])
  return { nodes: b.nodes, edges: b.edges, layoutNodes: b.layoutNodes, layoutEdges: b.layoutEdges }
}
