// Phase 5 of the Loom IDE redesign (docs/dev/loom-ide-redesign.md §6):
// workspace presets — saved dock layouts plus the visible panel set.
//
// Persists to `localStorage["loom.workspaces"]`. Ships five built-in
// presets (Author / Direct / Debug / Perform / Read); user-saved
// presets append onto the same list. Switching presets clears the
// dock and re-adds the preset's panels (or, for user presets, the
// dockview's serialized layout).

import { create } from 'zustand'
import type { DockviewApi, SerializedDockview } from 'dockview-react'
import {
  panelComponents,
  PANEL_ORDER,
  PANEL_TITLES,
  type PanelId,
} from '@/components/dock/panel-registry'
import { positionFor } from '@/components/dock/util'

const STORAGE_KEY = 'loom.workspaces'

export type WorkspacePreset = {
  id: string
  name: string
  /** True for the five shipped defaults — guards UI from delete/rename. */
  builtin: boolean
  /** Panels to open if `layout` is absent (used by the builtins). */
  visiblePanels?: PanelId[]
  /** Dockview's full serialized layout — used for user-saved presets. */
  layout?: SerializedDockview
}

const BUILTIN_PRESETS: WorkspacePreset[] = [
  {
    id: 'builtin-author',
    name: 'Author',
    builtin: true,
    visiblePanels: ['files', 'editor', 'canvas'],
  },
  {
    id: 'builtin-direct',
    name: 'Direct',
    builtin: true,
    visiblePanels: ['transcript', 'choices', 'world'],
  },
  {
    id: 'builtin-debug',
    name: 'Debug',
    builtin: true,
    visiblePanels: ['editor', 'timeline', 'ledger', 'world', 'detail'],
  },
  {
    id: 'builtin-perform',
    name: 'Perform',
    builtin: true,
    visiblePanels: ['transcript', 'choices', 'graph'],
  },
  {
    id: 'builtin-read',
    name: 'Read',
    builtin: true,
    visiblePanels: ['transcript'],
  },
]

type StoredState = {
  custom: WorkspacePreset[]
  active: string | null
}

function loadFromStorage(): StoredState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { custom: [], active: null }
    const parsed = JSON.parse(raw) as Partial<StoredState>
    return {
      custom: Array.isArray(parsed.custom) ? parsed.custom : [],
      active: typeof parsed.active === 'string' ? parsed.active : null,
    }
  } catch {
    return { custom: [], active: null }
  }
}

function persist(state: StoredState): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
  } catch {
    // Quota / private mode — preset persistence is best-effort.
  }
}

type PresetState = {
  custom: WorkspacePreset[]
  active: string | null
  /** All presets the UI should show — builtins first, customs after. */
  all(): WorkspacePreset[]
  apply(api: DockviewApi, id: string): void
  saveCurrent(api: DockviewApi, name: string): WorkspacePreset
  delete(id: string): void
  rename(id: string, name: string): void
}

export const usePresets = create<PresetState>((set, get) => {
  const initial = loadFromStorage()
  return {
    custom: initial.custom,
    active: initial.active,
    all() {
      return [...BUILTIN_PRESETS, ...get().custom]
    },
    apply(api, id) {
      const preset = get().all().find((p) => p.id === id)
      if (!preset) return
      api.clear()
      if (preset.layout) {
        // Restore user-captured layout verbatim.
        try {
          api.fromJSON(preset.layout)
        } catch {
          // Fall through to the visiblePanels path below.
          applyVisible(api, preset.visiblePanels ?? [])
        }
      } else {
        applyVisible(api, preset.visiblePanels ?? [])
      }
      set({ active: id })
      persist({ custom: get().custom, active: id })
    },
    saveCurrent(api, name) {
      const layout = api.toJSON()
      const preset: WorkspacePreset = {
        id: `user-${Date.now().toString(36)}`,
        name,
        builtin: false,
        layout,
      }
      const custom = [...get().custom, preset]
      set({ custom, active: preset.id })
      persist({ custom, active: preset.id })
      return preset
    },
    delete(id) {
      const custom = get().custom.filter((p) => p.id !== id)
      const active = get().active === id ? null : get().active
      set({ custom, active })
      persist({ custom, active })
    },
    rename(id, name) {
      const custom = get().custom.map((p) =>
        p.id === id ? { ...p, name } : p,
      )
      set({ custom })
      persist({ custom, active: get().active })
    },
  }
})

/**
 * Add each panel in `ids` in PANEL_ORDER. The dock's positioning
 * rules (in DockShell.tsx) place each one relative to those already
 * present — so ordering matters. Skips panels that aren't in the
 * registry (defensive against stale localStorage entries from older
 * builds).
 */
function applyVisible(api: DockviewApi, ids: PanelId[]): void {
  const ordered = PANEL_ORDER.filter((p) => ids.includes(p))
  for (const id of ordered) {
    if (!(id in panelComponents)) continue
    if (api.getPanel(id)) continue
    api.addPanel({
      id,
      component: id,
      title: PANEL_TITLES[id],
      position: positionFor(api, id),
    })
  }
}
