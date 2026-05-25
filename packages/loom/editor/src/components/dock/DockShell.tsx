import { useCallback, useEffect, useRef, useState } from 'react'
import clsx from 'clsx'
import {
  DockviewApi,
  DockviewReact,
  themeAbyss,
  type AddPanelPositionOptions,
  type DockviewReadyEvent,
} from 'dockview-react'
import { panelComponents, PANEL_ORDER, PANEL_TITLES, type PanelId } from './panel-registry'

const SHORTCUTS: Record<PanelId, { combo: string; label: string }> = {
  files: { combo: 'mod+b', label: '⌘B' },
  search: { combo: 'mod+shift+f', label: '⌘⇧F' },
  editor: { combo: 'mod+1', label: '⌘1' },
  canvas: { combo: 'mod+j', label: '⌘J' },
  cloud: { combo: 'mod+k', label: '⌘K' },
  remote: { combo: 'mod+2', label: '⌘2' },
}

function positionFor(api: DockviewApi, id: PanelId): AddPanelPositionOptions | undefined {
  const editor = api.getPanel('editor')
  const canvas = api.getPanel('canvas')
  const files = api.getPanel('files')
  const remote = api.getPanel('remote')

  if (id === 'files') {
    const ref = editor ?? canvas
    return ref ? { referencePanel: ref.id, direction: 'left' } : undefined
  }
  if (id === 'search') {
    if (files) return { referencePanel: files.id, direction: 'within' }
    const ref = editor ?? canvas
    return ref ? { referencePanel: ref.id, direction: 'left' } : undefined
  }
  if (id === 'cloud') {
    if (files) return { referencePanel: files.id, direction: 'within' }
    const ref = editor ?? canvas
    return ref ? { referencePanel: ref.id, direction: 'left' } : undefined
  }
  if (id === 'editor') {
    if (canvas) return { referencePanel: canvas.id, direction: 'above' }
    if (files) return { referencePanel: files.id, direction: 'right' }
    return undefined
  }
  if (id === 'remote') {
    if (editor) return { referencePanel: editor.id, direction: 'within' }
    if (canvas) return { referencePanel: canvas.id, direction: 'above' }
    return undefined
  }
  // canvas
  if (remote) return { referencePanel: remote.id, direction: 'below' }
  if (editor) return { referencePanel: editor.id, direction: 'below' }
  if (files) return { referencePanel: files.id, direction: 'right' }
  return undefined
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
  return (
    <svg {...common} aria-hidden>
      <circle cx="6" cy="6" r="2.5" />
      <circle cx="18" cy="6" r="2.5" />
      <circle cx="12" cy="18" r="2.5" />
      <path d="M7.7 7.7 11 16.2" />
      <path d="M16.3 7.7 13 16.2" />
      <path d="M8 6h8" />
    </svg>
  )
}

export function DockShell() {
  const apiRef = useRef<DockviewApi | null>(null)
  const sizesRef = useRef<Partial<Record<PanelId, PanelSize>>>({})
  const [openPanels, setOpenPanels] = useState<Set<PanelId>>(new Set(['files', 'editor', 'canvas']))

  const onReady = useCallback((event: DockviewReadyEvent) => {
    apiRef.current = event.api

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
  }, [])

  const toggle = useCallback((id: PanelId) => {
    const api = apiRef.current
    if (!api) return
    if (api.getPanel(id)) closePanel(api, id, sizesRef.current)
    else openPanel(api, id, sizesRef.current)
  }, [])

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
