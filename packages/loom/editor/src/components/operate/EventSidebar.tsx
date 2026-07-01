//! Operate mode — left rail: launch an event and control its lifecycle, with
//! the passcodes + a join QR to share with the room.

import { useOperate } from '@/store/operate'

function CodeRow({ label, code }: { label: string; code: string }) {
  return (
    <div className="flex items-center justify-between rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2">
      <div>
        <div className="text-[10px] uppercase tracking-wide text-zinc-500">{label}</div>
        <div className="font-mono text-base tracking-widest text-zinc-100">{code}</div>
      </div>
      <button
        onClick={() => void navigator.clipboard?.writeText(code)}
        className="rounded px-2 py-1 text-xs text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200"
        title="Copy"
      >
        copy
      </button>
    </div>
  )
}

export function EventSidebar() {
  const { event, phase, busy, error, launch, pause, resume, end } = useOperate()

  const phaseTone =
    phase === 'open' ? 'bg-emerald-900 text-emerald-300' : phase === 'paused' ? 'bg-amber-900 text-amber-300' : 'bg-zinc-800 text-zinc-400'

  return (
    <div className="h-full w-full overflow-auto bg-zinc-950 p-3 text-zinc-100">
      <div className="mb-3 flex items-center justify-between">
        <div className="text-sm font-semibold">Event</div>
        <span className={`rounded-full px-2 py-0.5 text-[10px] uppercase tracking-wide ${phaseTone}`}>{phase}</span>
      </div>

      {error && <div className="mb-2 rounded bg-red-950 px-2 py-1 text-xs text-red-300">{error}</div>}

      {!event ? (
        <div className="flex flex-col gap-2">
          <p className="text-xs text-zinc-500">
            Launch a private preview to test on the server, or go live so guests can join by code.
          </p>
          <button
            onClick={() => void launch('preview')}
            disabled={busy}
            className="rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-200 hover:bg-zinc-800 disabled:opacity-50"
          >
            Launch preview
          </button>
          <button
            onClick={() => void launch('live')}
            disabled={busy}
            className="rounded-lg bg-emerald-600 px-3 py-2 text-sm font-medium text-white hover:bg-emerald-500 disabled:opacity-50"
          >
            Go live
          </button>
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          <div className="text-xs text-zinc-500">
            Mode: <span className="text-zinc-300">{event.mode}</span>
          </div>

          <CodeRow label="Guest event code" code={event.codes.event} />
          <CodeRow label="Performer code" code={event.codes.prime} />
          <CodeRow label="Moderator code" code={event.codes.mod} />

          <div className="rounded-lg border border-zinc-800 bg-white p-2">
            {/* QR of the join URL — a phone camera drops guests straight in. */}
            <img
              src={`/api/qr?text=${encodeURIComponent(event.joinUrl)}`}
              alt="Join QR"
              className="mx-auto block h-40 w-40"
            />
          </div>
          <a href={event.joinUrl} target="_blank" rel="noreferrer" className="truncate text-center text-xs text-indigo-400 hover:underline">
            {event.joinUrl}
          </a>

          <div className="flex gap-2">
            {phase === 'open' ? (
              <button onClick={() => void pause()} disabled={busy} className="flex-1 rounded-lg border border-zinc-700 px-3 py-2 text-sm hover:bg-zinc-800 disabled:opacity-50">
                Pause
              </button>
            ) : (
              <button onClick={() => void resume()} disabled={busy} className="flex-1 rounded-lg bg-emerald-600 px-3 py-2 text-sm font-medium text-white hover:bg-emerald-500 disabled:opacity-50">
                Resume
              </button>
            )}
            <button
              onClick={() => window.confirm('End this event? Guests will be disconnected.') && void end()}
              disabled={busy}
              className="flex-1 rounded-lg border border-red-900 px-3 py-2 text-sm text-red-300 hover:bg-red-950 disabled:opacity-50"
            >
              End
            </button>
          </div>

          <div className="flex flex-col gap-1 border-t border-zinc-800 pt-2 text-xs">
            <a href={event.joinUrl} target="_blank" rel="noreferrer" className="text-zinc-400 hover:text-zinc-200">
              → Open guest view
            </a>
            <a href={`/e/${event.id}/console`} target="_blank" rel="noreferrer" className="text-zinc-400 hover:text-zinc-200">
              → Operator console
            </a>
          </div>
        </div>
      )}
    </div>
  )
}
