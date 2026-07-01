// Phase 1 of the Loom IDE redesign v2 (docs/dev/loom-ide-redesign.md
// Part II): the Studio shell. Replaces `DockShell` (activity bar +
// free-docking dockview) with five fixed, resizable region layouts
// switched from the bottom Mode Bar. ⌘1..⌘5 jump between modes.
//
// The shell is generic: a horizontal Allotment [left | center | tray]
// with an optional vertical [stage / timeline] split in the center.
// Region *contents* live in `regions.tsx`, keyed by the active mode.

import { useEffect } from 'react'
import { Allotment, LayoutPriority } from 'allotment'
import 'allotment/dist/style.css'
import { useMode, MODES, type Mode, type ModeUi } from '@/store/mode'
import { LeftRail, CenterStage, TimelineDock, Region } from './regions'
import { PropertiesTray } from './PropertiesTray'
import { ModeBar } from './ModeBar'

export function StudioShell() {
  const mode = useMode((s) => s.mode)
  const ui = useMode((s) => s.ui[s.mode])
  const setMode = useMode((s) => s.setMode)
  const setUi = useMode((s) => s.setUi)
  const hasTimeline = MODES.find((m) => m.id === mode)?.hasTimeline ?? false

  // ⌘1..⌘5 (or Ctrl) switch modes. Plain modifier only — Alt/Shift
  // combos stay free for other handlers.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey
      if (!mod || e.altKey || e.shiftKey) return
      const idx = ['1', '2', '3', '4', '5'].indexOf(e.key)
      if (idx < 0 || idx >= MODES.length) return
      e.preventDefault()
      setMode(MODES[idx].id)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [setMode])

  return (
    <div className="h-full w-full flex flex-col">
      <div className="flex-1 min-h-0">
        {/* Re-key per mode so each mode's persisted sizes seed via
            defaultSizes; tray/rail toggles keep the same instance so
            allotment remembers their widths. */}
        <ModeLayout
          key={mode}
          mode={mode}
          ui={ui}
          hasTimeline={hasTimeline}
          onCols={(cols) => setUi(mode, { cols })}
          onRows={(rows) => setUi(mode, { rows })}
        />
      </div>
      <ModeBar />
    </div>
  )
}

function ModeLayout({
  mode,
  ui,
  hasTimeline,
  onCols,
  onRows,
}: {
  mode: Mode
  ui: ModeUi
  hasTimeline: boolean
  onCols: (cols: [number, number, number]) => void
  onRows: (rows: [number, number]) => void
}) {
  return (
    <Allotment
      defaultSizes={ui.cols}
      proportionalLayout={false}
      // Persist only clean, fully-visible samples; skip the 0-sized
      // entries allotment reports while a pane is collapsed.
      onChange={(s) => {
        if (s.length === 3 && s.every((n) => n > 0)) {
          onCols([s[0], s[1], s[2]])
        }
      }}
    >
      <Allotment.Pane minSize={160} preferredSize={ui.cols[0]} visible={ui.railOpen} snap>
        <Region>
          <LeftRail mode={mode} />
        </Region>
      </Allotment.Pane>

      <Allotment.Pane minSize={320} priority={LayoutPriority.High}>
        {hasTimeline ? (
          <Allotment
            vertical
            defaultSizes={ui.rows}
            proportionalLayout={false}
            onChange={(s) => {
              if (s.length === 2 && s.every((n) => n > 0)) onRows([s[0], s[1]])
            }}
          >
            <Allotment.Pane minSize={140} priority={LayoutPriority.High}>
              <Region>
                <CenterStage mode={mode} />
              </Region>
            </Allotment.Pane>
            <Allotment.Pane minSize={100} preferredSize={ui.rows[1]} snap>
              <Region>
                <TimelineDock mode={mode} />
              </Region>
            </Allotment.Pane>
          </Allotment>
        ) : (
          <Region>
            <CenterStage mode={mode} />
          </Region>
        )}
      </Allotment.Pane>

      <Allotment.Pane minSize={240} preferredSize={ui.cols[2]} visible={ui.trayOpen} snap>
        <Region>
          <PropertiesTray mode={mode} />
        </Region>
      </Allotment.Pane>
    </Allotment>
  )
}
