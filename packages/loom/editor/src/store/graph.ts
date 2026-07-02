// State for the Editing-mode node editor (the story-graph canvas).
//
// View stack (project ⇄ beat drill-in), node selection, overlay toggles,
// per-project manual layout overrides (persisted), and the runtime
// overlay contract that Run mode (and a future local simulator) feeds —
// all owned here so the canvas, the Story Bin, the dock strip, and the
// Properties tray stay in lockstep.

import { create } from 'zustand'

/** What the canvas is showing: the whole project, or inside one beat. */
export type GraphView = { kind: 'project' } | { kind: 'beat'; beatKey: string }

export type GraphOverlays = {
  /** Character→beat hook routing (`on scan guest` → beat). */
  hooks: boolean
  /** Entity nodes + cast / setting / member / is / owns / contains edges. */
  entities: boolean
  /** Edge labels (choice text, conditions). */
  labels: boolean
}

/** Manual node-position overrides, keyed by node id. */
export type LayoutOverrides = Record<string, { x: number; y: number }>

/**
 * Live-run overlay: beat keys → visit counts, plus the most recently
 * entered beat (pulsed on the canvas). Fed by the operate store's mod
 * feed in Run mode; a local simulator drives the same contract.
 */
export type RuntimeOverlay = {
  visits: Record<string, number>
  current: string | null
}

const EMPTY_RUNTIME: RuntimeOverlay = { visits: {}, current: null }

type GraphState = {
  view: GraphView
  /** Selected canvas node id (beat key or entity id). */
  selected: string | null
  overlays: GraphOverlays
  /** Per-project manual position overrides (projectKey → overrides). */
  layouts: Record<string, LayoutOverrides>
  runtime: RuntimeOverlay
  search: string

  openProject(): void
  openBeat(beatKey: string): void
  select(id: string | null): void
  setOverlay(key: keyof GraphOverlays, on: boolean): void
  setSearch(q: string): void
  moveNode(projectKey: string, id: string, pos: { x: number; y: number }): void
  resetLayout(projectKey: string): void
  runtimeEnter(beatKey: string): void
  runtimeReset(): void
}

const LS_KEY = 'loom.graph.layouts'

function loadLayouts(): Record<string, LayoutOverrides> {
  try {
    const raw = localStorage.getItem(LS_KEY)
    return raw ? (JSON.parse(raw) as Record<string, LayoutOverrides>) : {}
  } catch {
    return {}
  }
}

function persist(layouts: Record<string, LayoutOverrides>): void {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(layouts))
  } catch {
    // quota / private mode — layout stays session-local
  }
}

export const useGraph = create<GraphState>((set) => ({
  view: { kind: 'project' },
  selected: null,
  overlays: { hooks: true, entities: false, labels: true },
  layouts: loadLayouts(),
  runtime: EMPTY_RUNTIME,
  search: '',

  openProject: () => set({ view: { kind: 'project' } }),
  openBeat: (beatKey) => set({ view: { kind: 'beat', beatKey }, selected: beatKey }),
  select: (id) => set({ selected: id }),
  setOverlay: (key, on) => set((s) => ({ overlays: { ...s.overlays, [key]: on } })),
  setSearch: (q) => set({ search: q }),
  moveNode: (projectKey, id, pos) =>
    set((s) => {
      const forProject = { ...(s.layouts[projectKey] ?? {}), [id]: pos }
      const layouts = { ...s.layouts, [projectKey]: forProject }
      persist(layouts)
      return { layouts }
    }),
  resetLayout: (projectKey) =>
    set((s) => {
      const layouts = { ...s.layouts }
      delete layouts[projectKey]
      persist(layouts)
      return { layouts }
    }),
  runtimeEnter: (beatKey) =>
    set((s) => ({
      runtime: {
        visits: { ...s.runtime.visits, [beatKey]: (s.runtime.visits[beatKey] ?? 0) + 1 },
        current: beatKey,
      },
    })),
  runtimeReset: () => set({ runtime: EMPTY_RUNTIME }),
}))
