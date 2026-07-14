// Phase 1 of the Loom IDE redesign v2 (docs/dev/loom-ide-redesign.md
// Part II): the modal topology. Replaces the free-docking `dockview`
// shell with fixed, purpose-built layouts switched from a bottom
// Mode Bar. This store owns the active mode + per-mode region sizes,
// persisted to localStorage["loom.studio"].

import { create } from 'zustand'

// Three modes (v3 consolidation): Writing is the whole authoring
// surface — text editor AND the story-graph node editor side by side;
// Run is the whole rehearsal/moderation cockpit — the local simulator
// and the live event behind one source switch; Deploy is the live
// event's lifecycle + admin controls (launch, codes/QR, pause/end).
export type Mode = 'writing' | 'run' | 'deploy'

export type ModeDescriptor = {
  id: Mode
  label: string
  /** Keybinding hint shown on the Mode Bar (⌘1..⌘3). */
  hint: string
  /** Whether the bottom Timeline dock is present in this mode. */
  hasTimeline: boolean
}

/** Ordered left→right as they appear on the Mode Bar; index ↔ ⌘1..⌘3. */
export const MODES: ModeDescriptor[] = [
  { id: 'writing', label: 'Writing', hint: '⌘1', hasTimeline: true },
  { id: 'run', label: 'Run', hint: '⌘2', hasTimeline: false },
  { id: 'deploy', label: 'Deploy', hint: '⌘3', hasTimeline: false },
]

/** The pre-v3 mode ids still sitting in persisted storage. */
const LEGACY_MODES: Record<string, Mode> = {
  editing: 'writing',
  sim: 'run',
  operate: 'run',
}

export type ModeUi = {
  /** Horizontal split sizes: [leftRail, center, propertiesTray]. */
  cols: [number, number, number]
  /** Vertical split sizes inside center: [stage, timeline]. */
  rows: [number, number]
  /** Writing-mode center split: [editor, graph]. */
  split: [number, number]
  /** Writing-mode: whether the story-graph pane is showing (⌘\). */
  graphOpen: boolean
  trayOpen: boolean
  railOpen: boolean
}

const STORAGE_KEY = 'loom.studio'

function defaultUi(): Record<Mode, ModeUi> {
  return {
    // Writing: files/story-bin rail · editor ⇄ story graph split · tray.
    writing: {
      cols: [240, 1000, 320],
      rows: [560, 180],
      split: [520, 620],
      graphOpen: true,
      trayOpen: true,
      railOpen: true,
    },
    // Run: rooms rail · sim/live cockpit · inspector tray.
    run: { cols: [300, 820, 340], rows: [620, 200], split: [520, 620], graphOpen: true, trayOpen: true, railOpen: true },
    // Deploy: launch/lifecycle/admin stage · inspector tray (no rail).
    deploy: { cols: [260, 900, 340], rows: [620, 200], split: [520, 620], graphOpen: true, trayOpen: true, railOpen: false },
  }
}

/** Clamp persisted sizes back to sane minimums (guards stale storage). */
function sanitize(ui: Partial<Record<Mode, Partial<ModeUi>>> | undefined): Record<Mode, ModeUi> {
  const base = defaultUi()
  const out = {} as Record<Mode, ModeUi>
  for (const id of Object.keys(base) as Mode[]) {
    const u = ui?.[id]
    const b = base[id]
    out[id] = {
      cols: [
        Math.max(160, u?.cols?.[0] ?? b.cols[0]),
        Math.max(320, u?.cols?.[1] ?? b.cols[1]),
        Math.max(240, u?.cols?.[2] ?? b.cols[2]),
      ],
      rows: [
        Math.max(120, u?.rows?.[0] ?? b.rows[0]),
        Math.max(100, u?.rows?.[1] ?? b.rows[1]),
      ],
      split: [
        Math.max(280, u?.split?.[0] ?? b.split[0]),
        Math.max(280, u?.split?.[1] ?? b.split[1]),
      ],
      graphOpen: u?.graphOpen ?? b.graphOpen,
      trayOpen: u?.trayOpen ?? b.trayOpen,
      railOpen: u?.railOpen ?? b.railOpen,
    }
  }
  return out
}

type Persisted = { mode: Mode; ui: Record<Mode, ModeUi> }

function load(): Persisted {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { mode: 'writing', ui: defaultUi() }
    const p = JSON.parse(raw) as Partial<Persisted>
    const stored = p.mode as string | undefined
    const mode = MODES.some((m) => m.id === stored)
      ? (stored as Mode)
      : (LEGACY_MODES[stored ?? ''] ?? 'writing')
    return { mode, ui: sanitize(p.ui) }
  } catch {
    return { mode: 'writing', ui: defaultUi() }
  }
}

function persist(mode: Mode, ui: Record<Mode, ModeUi>): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ mode, ui }))
  } catch {
    // Quota / private mode — layout persistence is best-effort.
  }
}

type ModeState = {
  mode: Mode
  ui: Record<Mode, ModeUi>
  setMode(m: Mode): void
  setUi(m: Mode, patch: Partial<ModeUi>): void
  toggleTray(): void
  toggleRail(): void
  /** Reset one mode's region sizes + visibility to the shipped defaults. */
  reset(m: Mode): void
}

export const useMode = create<ModeState>((set, get) => {
  const initial = load()
  return {
    mode: initial.mode,
    ui: initial.ui,
    setMode: (mode) => {
      set({ mode })
      persist(mode, get().ui)
    },
    setUi: (m, patch) => {
      const ui = { ...get().ui, [m]: { ...get().ui[m], ...patch } }
      set({ ui })
      persist(get().mode, ui)
    },
    toggleTray: () => {
      const m = get().mode
      get().setUi(m, { trayOpen: !get().ui[m].trayOpen })
    },
    toggleRail: () => {
      const m = get().mode
      get().setUi(m, { railOpen: !get().ui[m].railOpen })
    },
    reset: (m) => {
      const ui = { ...get().ui, [m]: defaultUi()[m] }
      set({ ui })
      persist(get().mode, ui)
    },
  }
})
