// Phase 1 of the Loom IDE redesign v2 (docs/dev/loom-ide-redesign.md
// Part II): the modal topology. Replaces the free-docking `dockview`
// shell with five fixed, purpose-built layouts switched from a bottom
// Mode Bar. This store owns the active mode + per-mode region sizes,
// persisted to localStorage["loom.studio"].

import { create } from 'zustand'

export type Mode =
  | 'writing'
  | 'editing'
  | 'simulating'
  | 'performing'
  | 'production'

export type ModeDescriptor = {
  id: Mode
  label: string
  /** Keybinding hint shown on the Mode Bar (⌘1..⌘5). */
  hint: string
  /** Whether the bottom Timeline dock is present in this mode. */
  hasTimeline: boolean
}

/** Ordered left→right as they appear on the Mode Bar; index ↔ ⌘1..⌘5. */
export const MODES: ModeDescriptor[] = [
  { id: 'writing', label: 'Writing', hint: '⌘1', hasTimeline: false },
  { id: 'editing', label: 'Editing', hint: '⌘2', hasTimeline: true },
  { id: 'simulating', label: 'Simulating', hint: '⌘3', hasTimeline: true },
  { id: 'performing', label: 'Performing', hint: '⌘4', hasTimeline: true },
  { id: 'production', label: 'Production', hint: '⌘5', hasTimeline: false },
]

export type ModeUi = {
  /** Horizontal split sizes: [leftRail, center, propertiesTray]. */
  cols: [number, number, number]
  /** Vertical split sizes inside center: [stage, timeline]. */
  rows: [number, number]
  trayOpen: boolean
  railOpen: boolean
}

const STORAGE_KEY = 'loom.studio'

function defaultUi(): Record<Mode, ModeUi> {
  return {
    writing: { cols: [260, 960, 320], rows: [620, 200], trayOpen: true, railOpen: true },
    editing: { cols: [240, 780, 360], rows: [400, 320], trayOpen: true, railOpen: true },
    simulating: { cols: [260, 760, 320], rows: [430, 250], trayOpen: true, railOpen: true },
    performing: { cols: [240, 780, 360], rows: [460, 230], trayOpen: true, railOpen: true },
    production: { cols: [320, 860, 300], rows: [620, 200], trayOpen: true, railOpen: true },
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
    const mode = MODES.some((m) => m.id === p.mode) ? (p.mode as Mode) : 'writing'
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
  }
})
