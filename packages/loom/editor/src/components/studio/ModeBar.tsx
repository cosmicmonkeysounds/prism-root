// The bottom Mode Bar — the primary navigation of the IDE, DaVinci
// Resolve-style. Three modes: Writing (text + story graph), Run
// (rehearse on the local sim / moderate the live event), Deploy (the
// live event's lifecycle + admin controls). Also hosts the left-rail /
// properties-tray collapse toggles for the active mode.

import clsx from 'clsx'
import { useMode, MODES, type Mode } from '@/store/mode'

export function ModeBar() {
  const mode = useMode((s) => s.mode)
  const setMode = useMode((s) => s.setMode)
  const railOpen = useMode((s) => s.ui[s.mode].railOpen)
  const trayOpen = useMode((s) => s.ui[s.mode].trayOpen)
  const toggleRail = useMode((s) => s.toggleRail)
  const toggleTray = useMode((s) => s.toggleTray)

  return (
    <nav
      aria-label="Editor mode"
      data-testid="mode-bar"
      className="h-12 shrink-0 flex items-center border-t border-white/10 bg-zinc-950 px-2"
    >
      <SideToggle side="left" active={railOpen} onClick={toggleRail} />
      <div className="flex-1 flex items-center justify-center gap-1">
        {MODES.map((m) => {
          const active = m.id === mode
          return (
            <button
              key={m.id}
              type="button"
              data-testid={`mode-${m.id}`}
              aria-pressed={active}
              onClick={() => setMode(m.id)}
              title={`${m.label} (${m.hint})`}
              className={clsx(
                'h-9 px-3 rounded-md flex items-center gap-2 text-sm transition-colors',
                'focus:outline-none focus-visible:ring-1 focus-visible:ring-blue-400/60',
                active
                  ? 'bg-blue-500/15 text-blue-200 ring-1 ring-blue-400/40'
                  : 'text-zinc-400 hover:text-zinc-100 hover:bg-white/5',
              )}
            >
              <ModeGlyph id={m.id} />
              <span className="font-medium">{m.label}</span>
              <kbd
                className={clsx(
                  'text-[10px] font-mono',
                  active ? 'text-blue-300/70' : 'text-zinc-600',
                )}
              >
                {m.hint}
              </kbd>
            </button>
          )
        })}
      </div>
      <SideToggle side="right" active={trayOpen} onClick={toggleTray} />
    </nav>
  )
}

function SideToggle({
  side,
  active,
  onClick,
}: {
  side: 'left' | 'right'
  active: boolean
  onClick: () => void
}) {
  const x = side === 'left' ? 9 : 15
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      title={`Toggle ${side === 'left' ? 'left rail' : 'properties tray'}`}
      className={clsx(
        'h-9 w-9 grid place-items-center rounded-md transition-colors',
        active ? 'text-zinc-200' : 'text-zinc-600 hover:text-zinc-300',
      )}
    >
      <svg
        width="18"
        height="18"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        <rect x="3" y="4" width="18" height="16" rx="2" />
        <line x1={x} y1="4" x2={x} y2="20" />
      </svg>
    </button>
  )
}

function ModeGlyph({ id }: { id: Mode }) {
  const common = {
    width: 16,
    height: 16,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 1.75,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
  }
  switch (id) {
    case 'writing':
      return (
        <svg {...common} aria-hidden>
          <path d="M12 20h9" />
          <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
        </svg>
      )
    case 'run':
      // Play-in-a-circle — rehearse locally or moderate the live run.
      return (
        <svg {...common} aria-hidden>
          <circle cx="12" cy="12" r="9" />
          <path d="M10 8.5l6 3.5-6 3.5Z" />
        </svg>
      )
    case 'deploy':
      // Broadcast tower — launching + administering the live event.
      return (
        <svg {...common} aria-hidden>
          <circle cx="12" cy="12" r="2" />
          <path d="M16.24 7.76a6 6 0 0 1 0 8.49M7.76 16.24a6 6 0 0 1 0-8.49" />
          <path d="M19.07 4.93a10 10 0 0 1 0 14.14M4.93 19.07a10 10 0 0 1 0-14.14" />
        </svg>
      )
  }
}
