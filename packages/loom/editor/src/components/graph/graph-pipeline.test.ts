// The Editing-mode node-editor data pipeline, driven end-to-end over the
// reference scenario: core story graph → project flow (file containers,
// beats, entities, edges) → ELK layout, and the beat drill-in flow.
// Catches shape drift between the core graph and the canvas projection
// without needing a browser.

import { describe, expect, it } from 'vitest'
import { Workspace } from '@loom/core/lsp'
import { pathForUri } from '@/lib/lsp-client'
import { buildBeatFlow } from './beat-flow'
import { buildProjectFlow, END_ID, groupId } from './flow'
import { layeredLayout } from './layout'
import { FILE_COLLAPSED_H, FILE_COLLAPSED_W } from './metrics'

// Raw-import the reference scenario through vite's glob (browser-typed,
// no node builtins). `main.loom` must lead — the compiled model takes
// `entry:` from the earliest file.
const RAW = import.meta.glob('../../../../core/examples/escape-the-internet/**/*.loom', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>

function corpus(): Workspace {
  const ws = new Workspace()
  const entries = Object.entries(RAW)
    .map(([p, source]) => {
      const rel = p.split('escape-the-internet/')[1] ?? p
      return [rel, source] as [string, string]
    })
    .sort(([a], [b]) => {
      const am = a === 'main.loom' ? 0 : 1
      const bm = b === 'main.loom' ? 0 : 1
      return am - bm || a.localeCompare(b)
    })
  ws.updateMany(entries.map(([rel, source]) => [`inmemory:///${rel}`, source]))
  return ws
}

const OVERLAYS = { hooks: true, entities: false, labels: true }

describe('project flow projection', () => {
  const ws = corpus()
  const graph = ws.storyGraph()
  const flow = buildProjectFlow(graph, OVERLAYS)

  it('projects every beat as a node inside its file container', () => {
    const ids = new Set(flow.nodes.map((n) => n.id))
    for (const key of graph.beats.keys()) expect(ids.has(key)).toBe(true)
    // React Flow requires parents to precede children.
    const seen = new Set<string>()
    for (const n of flow.nodes) {
      if (n.parentId !== undefined) expect(seen.has(n.parentId)).toBe(true)
      seen.add(n.id)
    }
  })

  it('keeps every edge endpoint present (incl. hook sources)', () => {
    const ids = new Set(flow.nodes.map((n) => n.id))
    for (const e of flow.edges) {
      expect(ids.has(e.source)).toBe(true)
      expect(ids.has(e.target)).toBe(true)
    }
    // This scenario is a live event — it never `-> END`s.
    expect(graph.hasEnd).toBe(false)
    expect(ids.has(END_ID)).toBe(false)
    // Scanner props surface as hook sources even with entities off.
    expect(ids.has('character:Crawler')).toBe(true)
  })

  it('lays out every node with ELK (children relative to containers)', async () => {
    const { positions, groupSizes } = await layeredLayout(flow.layoutNodes, flow.layoutEdges, 'RIGHT')
    for (const n of flow.layoutNodes) {
      if (n.isGroup) {
        const size = groupSizes.get(n.id)
        expect(size).toBeDefined()
        expect(size!.width).toBeGreaterThan(0)
      } else {
        expect(positions.get(n.id)).toBeDefined()
      }
    }
  })
})

describe('collapsed file containers', () => {
  const ws = corpus()
  const graph = ws.storyGraph()
  // prison.loom receives cross-file hook routes (The_Banned →
  // banned_ghost, Hacker_Zero → hacker_tool) — a good collapse target.
  const prison = graph.files.find((f) => f.uri.endsWith('/beats/prison.loom'))!
  const fileNode = groupId(prison.uri)
  const flow = buildProjectFlow(graph, OVERLAYS, new Map(), new Set([pathForUri(prison.uri)]))

  it('emits the collapsed file as a compact childless leaf', () => {
    const node = flow.nodes.find((n) => n.id === fileNode)!
    expect((node.data as { collapsed?: boolean }).collapsed).toBe(true)
    expect(flow.nodes.some((n) => n.parentId === fileNode)).toBe(false)
    for (const key of prison.beats) expect(flow.nodes.some((n) => n.id === key)).toBe(false)
    // ELK sees a fixed-size leaf, not a group.
    const layout = flow.layoutNodes.find((n) => n.id === fileNode)!
    expect(layout.isGroup).toBeUndefined()
    expect(layout.width).toBe(FILE_COLLAPSED_W)
    expect(layout.height).toBe(FILE_COLLAPSED_H)
  })

  it('re-routes every edge touching the collapsed file onto its node', () => {
    const ids = new Set(flow.nodes.map((n) => n.id))
    const hidden = new Set([...prison.beats, ...prison.entities])
    for (const e of flow.edges) {
      expect(ids.has(e.source)).toBe(true)
      expect(ids.has(e.target)).toBe(true)
      expect(hidden.has(e.source)).toBe(false)
      expect(hidden.has(e.target)).toBe(false)
    }
    expect(flow.edges.some((e) => e.source === fileNode || e.target === fileNode)).toBe(true)
  })

  it('merges re-routed parallels into exactly one edge per pair', () => {
    // Expected pairs derived from the uncollapsed flow: each must
    // surface exactly once, counted when it aggregates 2+ raw edges.
    const raw = buildProjectFlow(graph, OVERLAYS)
    const hidden = new Set([...prison.beats, ...prison.entities])
    const rerouted = new Map<string, number>()
    for (const e of raw.edges) {
      const s = hidden.has(e.source) ? fileNode : e.source
      const t = hidden.has(e.target) ? fileNode : e.target
      if ((s === e.source && t === e.target) || s === t) continue
      rerouted.set(`${s}→${t}`, (rerouted.get(`${s}→${t}`) ?? 0) + 1)
    }
    expect(rerouted.size).toBeGreaterThan(0)
    for (const [pair, count] of rerouted) {
      const [s, t] = pair.split('→')
      const matches = flow.edges.filter((e) => e.source === s && e.target === t)
      expect(matches).toHaveLength(1)
      if (count > 1) expect(matches[0].label).toBe(`${count} links`)
    }
  })

  it('lays out with ELK as a root-level leaf', async () => {
    const { positions, groupSizes } = await layeredLayout(flow.layoutNodes, flow.layoutEdges, 'RIGHT')
    expect(positions.get(fileNode)).toBeDefined()
    expect(groupSizes.has(fileNode)).toBe(false)
  })
})

describe('beat drill-in flow', () => {
  const ws = corpus()
  const graph = ws.storyGraph()

  it('renders captcha_gate 1:1 — dialogue, choice fan-out, exits', () => {
    const beat = graph.beats.get('captcha_gate')!
    const body = ws.beatBody('captcha_gate')!
    const flow = buildBeatFlow(graph, beat, body)
    const types = flow.nodes.map((n) => n.type)
    expect(types).toContain('bodyStart')
    expect(types.filter((t) => t === 'bodyChoice').length).toBeGreaterThanOrEqual(2)
    const exits = flow.nodes.filter((n) => n.type === 'bodyExit')
    expect(exits.some((n) => (n.data as { resolved: string | null }).resolved === 'lockdown')).toBe(
      true,
    )
    // Every edge endpoint is a real node.
    const ids = new Set(flow.nodes.map((n) => n.id))
    for (const e of flow.edges) {
      expect(ids.has(e.source)).toBe(true)
      expect(ids.has(e.target)).toBe(true)
    }
  })

  it('renders an owned beat body through the raw-line lowering', () => {
    const beat = graph.beats.get('Sysadmin.interrogation')!
    const body = ws.beatBody('Sysadmin.interrogation')!
    const flow = buildBeatFlow(graph, beat, body)
    expect(flow.nodes.some((n) => n.type === 'bodyDialogue')).toBe(true)
    expect(flow.nodes.some((n) => n.type === 'bodyBranch')).toBe(true)
  })
})
