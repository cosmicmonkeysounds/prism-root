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
 * Live-run overlay: beat keys → visit counts, the most recently
 * entered beat (pulsed on the canvas), and best-effort edge traversal
 * counts (`from→to` between consecutively entered beats). Fed by the
 * operate store's mod feed in Run mode and by the local Sim-mode
 * simulator (`store/sim.ts`) — the identical contract.
 */
export type RuntimeOverlay = {
  visits: Record<string, number>
  current: string | null
  /** `${from}→${to}` → times the runtime moved between those beats. */
  traversed: Record<string, number>
}

/** The traversal key an edge decoration looks up. */
export const traversalKey = (from: string, to: string): string => `${from}→${to}`

const EMPTY_RUNTIME: RuntimeOverlay = { visits: {}, current: null, traversed: {} }

type GraphState = {
  view: GraphView
  /** Selected canvas node id (beat key / entity id / `file:` group). */
  selected: string | null
  /** Selected edge id (mutually exclusive with `selected`). */
  selectedEdge: string | null
  /**
   * A pending "bring this node into view" command (from the Story Bin,
   * the tray's link lists, search…). The canvas consumes it — retrying
   * across relayouts until the node exists — then clears it.
   */
  centerRequest: { id: string; token: number } | null
  /** Drill-in node currently inline-editing its source slice. */
  editingNode: string | null
  /** Project-view word block currently inline-editing its source slice. */
  editingBlock: { beatKey: string; index: number } | null
  /**
   * Beat keys whose project-view card is expanded to show every word
   * block of its body (the card stretches to fit; collapse restores
   * the compact preview). Session-local — layout, not document, state.
   */
  expanded: Record<string, true>
  /**
   * File containers collapsed to a compact header-only node (keyed by
   * path). Their beats/entities hide and edges re-route to the file
   * node. Session-local — layout, not document, state.
   */
  collapsedFiles: Record<string, true>
  overlays: GraphOverlays
  /** Per-project manual position overrides (projectKey → overrides). */
  layouts: Record<string, LayoutOverrides>
  runtime: RuntimeOverlay
  search: string

  openProject(): void
  openBeat(beatKey: string): void
  select(id: string | null): void
  selectEdge(id: string | null): void
  /** Select + ask the canvas to center/zoom on `id`. */
  reveal(id: string): void
  clearCenter(token: number): void
  setEditing(id: string | null): void
  setEditingBlock(v: { beatKey: string; index: number } | null): void
  /** Expand / collapse one beat card's word blocks. */
  toggleExpanded(beatKey: string): void
  /** Expand (`true`) or collapse (`false`) every beat card at once. */
  setAllExpanded(keys: string[], on: boolean): void
  /** Collapse / expand one file container. */
  toggleFileCollapsed(path: string): void
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
  selectedEdge: null,
  centerRequest: null,
  editingNode: null,
  editingBlock: null,
  expanded: {},
  collapsedFiles: {},
  overlays: { hooks: true, entities: false, labels: true },
  layouts: loadLayouts(),
  runtime: EMPTY_RUNTIME,
  search: '',

  openProject: () => set({ view: { kind: 'project' }, editingNode: null, editingBlock: null }),
  openBeat: (beatKey) =>
    set({
      view: { kind: 'beat', beatKey },
      selected: beatKey,
      selectedEdge: null,
      editingNode: null,
      editingBlock: null,
    }),
  select: (id) => set({ selected: id, selectedEdge: null }),
  selectEdge: (id) => set({ selectedEdge: id, selected: null }),
  reveal: (id) =>
    set((s) => ({
      selected: id,
      selectedEdge: null,
      centerRequest: { id, token: (s.centerRequest?.token ?? 0) + 1 },
    })),
  clearCenter: (token) =>
    set((s) => (s.centerRequest?.token === token ? { centerRequest: null } : {})),
  setEditing: (id) => set({ editingNode: id }),
  setEditingBlock: (v) => set({ editingBlock: v }),
  toggleExpanded: (beatKey) =>
    set((s) => {
      const expanded = { ...s.expanded }
      if (expanded[beatKey]) delete expanded[beatKey]
      else expanded[beatKey] = true
      return { expanded }
    }),
  setAllExpanded: (keys, on) =>
    set(() => {
      const expanded: Record<string, true> = {}
      if (on) for (const k of keys) expanded[k] = true
      return { expanded }
    }),
  toggleFileCollapsed: (path) =>
    set((s) => {
      const collapsedFiles = { ...s.collapsedFiles }
      if (collapsedFiles[path]) delete collapsedFiles[path]
      else collapsedFiles[path] = true
      return { collapsedFiles }
    }),
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
    set((s) => {
      // Best-effort traversal: mark the hop from the previous beat. A hook
      // may interleave unrelated beats, so this is a heat overlay, not an
      // exact trace — good enough to light the routes a run actually took.
      const traversed = { ...s.runtime.traversed }
      if (s.runtime.current !== null && s.runtime.current !== beatKey) {
        const k = traversalKey(s.runtime.current, beatKey)
        traversed[k] = (traversed[k] ?? 0) + 1
      }
      return {
        runtime: {
          visits: { ...s.runtime.visits, [beatKey]: (s.runtime.visits[beatKey] ?? 0) + 1 },
          current: beatKey,
          traversed,
        },
      }
    }),
  runtimeReset: () => set({ runtime: EMPTY_RUNTIME }),
}))
