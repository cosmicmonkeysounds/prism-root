import { useCallback, useEffect, useRef, useState } from 'react'
import clsx from 'clsx'
import {
  DockviewApi,
  DockviewReact,
  themeAbyss,
  type DockviewReadyEvent,
} from 'dockview-react'
import { panelComponents, PANEL_ORDER, PANEL_TITLES, type PanelId } from './panel-registry'
import { usePresets } from '@/store/presets'
import { positionFor, setActiveDockApi } from './util'

const SHORTCUTS: Record<PanelId, { combo: string; label: string }> = {
  files: { combo: 'mod+b', label: '⌘B' },
  search: { combo: 'mod+shift+f', label: '⌘⇧F' },
  editor: { combo: 'mod+1', label: '⌘1' },
  canvas: { combo: 'mod+j', label: '⌘J' },
  cloud: { combo: 'mod+k', label: '⌘K' },
  remote: { combo: 'mod+2', label: '⌘2' },
  play: { combo: 'mod+shift+p', label: '⌘⇧P' },
  transcript: { combo: 'mod+shift+t', label: '⌘⇧T' },
  choices: { combo: 'mod+shift+c', label: '⌘⇧C' },
  ledger: { combo: 'mod+shift+l', label: '⌘⇧L' },
  world: { combo: 'mod+shift+w', label: '⌘⇧W' },
  timeline: { combo: 'mod+shift+m', label: '⌘⇧M' },
  inspector: { combo: 'mod+shift+i', label: '⌘⇧I' },
  graph: { combo: 'mod+shift+g', label: '⌘⇧G' },
  detail: { combo: 'mod+shift+d', label: '⌘⇧D' },
  cast: { combo: 'mod+shift+a', label: '⌘⇧A' },
  booth: { combo: 'mod+shift+b', label: '⌘⇧B' },
  outline: { combo: 'mod+shift+o', label: '⌘⇧O' },
  references: { combo: 'mod+shift+r', label: '⌘⇧R' },
}

type PanelSize = { width: number; height: number }

function openPanel(
  api: DockviewApi,
  id: PanelId,
  sizes: Partial<Record<PanelId, PanelSize>>,
) {
  const existing = api.getPanel(id)
  if (existing) {
    existing.api.setActive()
    return
  }
  const size = sizes[id]
  api.addPanel({
    id,
    component: id,
    title: PANEL_TITLES[id],
    position: positionFor(api, id),
    initialWidth: size?.width,
    initialHeight: size?.height,
  })
}

function closePanel(
  api: DockviewApi,
  id: PanelId,
  sizes: Partial<Record<PanelId, PanelSize>>,
) {
  const panel = api.getPanel(id)
  if (!panel) return
  sizes[id] = { width: panel.api.width, height: panel.api.height }
  api.removePanel(panel)
}

function PanelIcon({ id }: { id: PanelId }) {
  // Lucide-style 20px icons; stroke inherits currentColor
  const common = {
    width: 20,
    height: 20,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 1.75,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
  }
  if (id === 'files') {
    return (
      <svg {...common} aria-hidden>
        <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
      </svg>
    )
  }
  if (id === 'search') {
    return (
      <svg {...common} aria-hidden>
        <circle cx="11" cy="11" r="6" />
        <path d="m20 20-3.5-3.5" />
      </svg>
    )
  }
  if (id === 'editor') {
    return (
      <svg {...common} aria-hidden>
        <path d="M4 4h16v16H4z" />
        <path d="M4 9h16" />
        <path d="M9 4v16" />
      </svg>
    )
  }
  if (id === 'cloud') {
    return (
      <svg {...common} aria-hidden>
        <path d="M7 18a5 5 0 1 1 .8-9.94A6 6 0 0 1 19 12a4 4 0 0 1-1 7.87" />
      </svg>
    )
  }
  if (id === 'remote') {
    return (
      <svg {...common} aria-hidden>
        <circle cx="12" cy="12" r="3" />
        <path d="M3 12a9 9 0 0 1 18 0" />
        <path d="M5 17a7 7 0 0 1 14 0" />
      </svg>
    )
  }
  if (id === 'play') {
    return (
      <svg {...common} aria-hidden>
        <polygon points="6 4 20 12 6 20 6 4" />
      </svg>
    )
  }
  if (id === 'transcript') {
    return (
      <svg {...common} aria-hidden>
        <path d="M4 6h16M4 11h16M4 16h10" />
      </svg>
    )
  }
  if (id === 'choices') {
    return (
      <svg {...common} aria-hidden>
        <path d="M9 6h11M9 12h11M9 18h11" />
        <circle cx="5" cy="6" r="1.5" />
        <circle cx="5" cy="12" r="1.5" />
        <circle cx="5" cy="18" r="1.5" />
      </svg>
    )
  }
  if (id === 'ledger') {
    return (
      <svg {...common} aria-hidden>
        <rect x="4" y="4" width="16" height="16" rx="1" />
        <path d="M4 9h16M4 14h16M9 4v16" />
      </svg>
    )
  }
  if (id === 'world') {
    return (
      <svg {...common} aria-hidden>
        <circle cx="12" cy="12" r="8" />
        <path d="M4 12h16M12 4c3 3 3 13 0 16M12 4c-3 3-3 13 0 16" />
      </svg>
    )
  }
  if (id === 'timeline') {
    return (
      <svg {...common} aria-hidden>
        <path d="M3 6h18M3 12h18M3 18h18" />
        <rect x="5" y="4" width="3" height="4" fill="currentColor" stroke="none" />
        <rect x="11" y="10" width="3" height="4" fill="currentColor" stroke="none" />
        <rect x="16" y="16" width="3" height="4" fill="currentColor" stroke="none" />
      </svg>
    )
  }
  if (id === 'inspector') {
    return (
      <svg {...common} aria-hidden>
        <circle cx="11" cy="11" r="6" />
        <path d="m20 20-3.5-3.5M9 11h4M11 9v4" />
      </svg>
    )
  }
  if (id === 'graph') {
    return (
      <svg {...common} aria-hidden>
        <circle cx="6" cy="6" r="2.5" />
        <circle cx="18" cy="6" r="2.5" />
        <circle cx="12" cy="18" r="2.5" />
        <path d="M7.7 7.7 11 16.2M16.3 7.7 13 16.2M8 6h8" />
      </svg>
    )
  }
  if (id === 'detail') {
    return (
      <svg {...common} aria-hidden>
        <rect x="4" y="4" width="16" height="16" rx="1" />
        <path d="M8 9h8M8 13h8M8 17h5" />
      </svg>
    )
  }
  if (id === 'cast') {
    return (
      <svg {...common} aria-hidden>
        <circle cx="9" cy="9" r="3" />
        <path d="M3 19a6 6 0 0 1 12 0" />
        <circle cx="17" cy="7" r="2" />
        <path d="M21 16a4 4 0 0 0-7-2.6" />
      </svg>
    )
  }
  if (id === 'booth') {
    return (
      <svg {...common} aria-hidden>
        <rect x="3" y="6" width="18" height="12" rx="1" />
        <circle cx="9" cy="12" r="2" />
        <path d="M15 10v4M18 9v6" />
      </svg>
    )
  }
  if (id === 'outline') {
    return (
      <svg {...common} aria-hidden>
        <path d="M4 6h6M4 10h10M4 14h6M4 18h12" />
      </svg>
    )
  }
  if (id === 'references') {
    return (
      <svg {...common} aria-hidden>
        <path d="M7 7h10v10H7z" />
        <path d="M3 3h8v4H3zM13 17h8v4h-8z" />
      </svg>
    )
  }
  return (
    <svg {...common} aria-hidden>
      <rect x="4" y="4" width="16" height="16" rx="2" />
    </svg>
  )
}

export function DockShell() {
  const apiRef = useRef<DockviewApi | null>(null)
  const sizesRef = useRef<Partial<Record<PanelId, PanelSize>>>({})
  const [openPanels, setOpenPanels] = useState<Set<PanelId>>(new Set(['files', 'editor', 'canvas']))

  const onReady = useCallback((event: DockviewReadyEvent) => {
    apiRef.current = event.api
    // Stash the live API so non-React surfaces (Phase 5 preset menu in
    // the status bar) can hand it to `usePresets.apply/saveCurrent`.
    setActiveDockApi(event.api)

    const filesWidth = Math.round(window.innerWidth / 4)
    const files = event.api.addPanel({
      id: 'files',
      component: 'files',
      title: PANEL_TITLES.files,
      initialWidth: filesWidth,
    })
    const editor = event.api.addPanel({
      id: 'editor',
      component: 'editor',
      title: PANEL_TITLES.editor,
      position: { referencePanel: files.id, direction: 'right' },
    })
    event.api.addPanel({
      id: 'canvas',
      component: 'canvas',
      title: PANEL_TITLES.canvas,
      position: { referencePanel: editor.id, direction: 'below' },
    })
    editor.api.setActive()

    const refresh = () => {
      const next = new Set<PanelId>()
      for (const id of PANEL_ORDER) if (event.api.getPanel(id)) next.add(id)
      setOpenPanels(next)
    }
    event.api.onDidAddPanel(refresh)
    event.api.onDidRemovePanel(refresh)

    // Track live size changes so the snapshot on close reflects the latest user resize.
    for (const id of PANEL_ORDER) {
      const panel = event.api.getPanel(id)
      if (!panel) continue
      panel.api.onDidDimensionsChange(({ width, height }) => {
        if (width > 0 && height > 0) sizesRef.current[id] = { width, height }
      })
    }
    event.api.onDidAddPanel((panel) => {
      const pid = panel.id as PanelId
      if (!PANEL_ORDER.includes(pid)) return
      panel.api.onDidDimensionsChange(({ width, height }) => {
        if (width > 0 && height > 0) sizesRef.current[pid] = { width, height }
      })
    })

    refresh()
  }, [])

  useEffect(() => () => {
    apiRef.current = null
    setActiveDockApi(null)
  }, [])

  const toggle = useCallback((id: PanelId) => {
    const api = apiRef.current
    if (!api) return
    if (api.getPanel(id)) closePanel(api, id, sizesRef.current)
    else openPanel(api, id, sizesRef.current)
  }, [])

  // Phase 5 — workspace presets. Cmd+Alt+1..5 cycles the five
  // builtins; user-saved presets are reachable from the StatusBar
  // switcher.
  const applyPreset = usePresets((s) => s.apply)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey
      if (!mod || !e.altKey || e.shiftKey) return
      const map: Record<string, string> = {
        '1': 'builtin-author',
        '2': 'builtin-direct',
        '3': 'builtin-debug',
        '4': 'builtin-perform',
        '5': 'builtin-read',
      }
      const id = map[e.key]
      if (!id) return
      e.preventDefault()
      const api = apiRef.current
      if (api) applyPreset(api, id)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [applyPreset])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey
      if (!mod || e.altKey) return
      const k = e.key.toLowerCase()
      let target: PanelId | null = null
      if (k === 'b' && !e.shiftKey) target = 'files'
      else if (k === 'f' && e.shiftKey) target = 'search'
      else if (k === 'j' && !e.shiftKey) target = 'canvas'
      else if (k === '1' && !e.shiftKey) target = 'editor'
      else if (k === '2' && !e.shiftKey) target = 'remote'
      else if (k === 'k' && !e.shiftKey) target = 'cloud'
      else if (k === 'p' && e.shiftKey) target = 'play'
      else if (k === 't' && e.shiftKey) target = 'transcript'
      else if (k === 'c' && e.shiftKey) target = 'choices'
      else if (k === 'l' && e.shiftKey) target = 'ledger'
      else if (k === 'w' && e.shiftKey) target = 'world'
      else if (k === 'm' && e.shiftKey) target = 'timeline'
      else if (k === 'i' && e.shiftKey) target = 'inspector'
      else if (k === 'g' && e.shiftKey) target = 'graph'
      else if (k === 'd' && e.shiftKey) target = 'detail'
      else if (k === 'a' && e.shiftKey) target = 'cast'
      else if (k === 'b' && e.shiftKey) target = 'booth'
      else if (k === 'o' && e.shiftKey) target = 'outline'
      else if (k === 'r' && e.shiftKey) target = 'references'
      if (!target) return
      e.preventDefault()
      toggle(target)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [toggle])

  return (
    <div className="h-full w-full flex">
      <nav
        data-testid="activity-bar"
        aria-label="Panels"
        className="flex flex-col w-11 shrink-0 bg-zinc-950 border-r border-white/10"
      >
        {PANEL_ORDER.map((id) => {
          const isOpen = openPanels.has(id)
          return (
            <button
              key={id}
              type="button"
              data-testid={`activity-${id}`}
              aria-pressed={isOpen}
              aria-label={`${PANEL_TITLES[id]} (${SHORTCUTS[id].label})`}
              title={`${PANEL_TITLES[id]}  ${SHORTCUTS[id].label}`}
              onClick={() => toggle(id)}
              className={clsx(
                'relative h-11 flex items-center justify-center transition-colors',
                'focus:outline-none focus-visible:ring-1 focus-visible:ring-blue-400/60',
                isOpen
                  ? 'text-white'
                  : 'text-zinc-500 hover:text-zinc-200',
              )}
            >
              <span
                aria-hidden
                className={clsx(
                  'absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded-r',
                  isOpen ? 'bg-blue-400' : 'bg-transparent',
                )}
              />
              <PanelIcon id={id} />
            </button>
          )
        })}
      </nav>

      <div className="flex-1 min-w-0 h-full">
        <DockviewReact
          components={panelComponents}
          onReady={onReady}
          theme={themeAbyss}
          disableFloatingGroups={false}
        />
      </div>
    </div>
  )
}
