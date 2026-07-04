//! The cockpit's left rail — a persistent status strip, the perspective
//! ("viewing as") lens picker, and the Rooms navigator. Clicking a room
//! jumps to the Chat page focused on it, so the channel list is always
//! one click away regardless of the active page. Shared by Sim mode
//! (local simulator) and Run mode (live event).

import clsx from 'clsx'
import { CockpitPhase, CockpitTab, OPERATOR_LENS, useCockpit } from '@/store/cockpit'
import { useRooms, useRoomCounts } from './rooms'
import { channelGlyph } from './format'

/** The "viewing as" lens picker: Operator · every guest · every character. */
function PerspectiveSelect() {
  const roster = useCockpit((s) => s.roster)
  const cast = useCockpit((s) => s.cast)
  const perspective = useCockpit((s) => s.perspective)
  const setPerspective = useCockpit((s) => s.setPerspective)
  return (
    <select
      value={perspective}
      onChange={(e) => setPerspective(e.target.value)}
      className="mt-2 w-full rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-xs text-zinc-200 outline-none focus:border-indigo-500"
      title="Viewing as"
      data-testid="cockpit-perspective"
    >
      <option value={OPERATOR_LENS}>👁 Operator (everything)</option>
      {roster.length > 0 && (
        <optgroup label="Guests">
          {roster.map((r) => (
            <option key={r.id} value={r.id}>
              {r.name} ({r.id})
            </option>
          ))}
        </optgroup>
      )}
      {cast.length > 0 && (
        <optgroup label="Cast">
          {cast.map((c) => (
            <option key={c.id} value={c.id}>
              {c.id}
            </option>
          ))}
        </optgroup>
      )}
    </select>
  )
}

export function CockpitRail({ statusText }: { statusText?: string }) {
  const phase = useCockpit((s) => s.phase)
  const connected = useCockpit((s) => s.connected)
  const live = useCockpit((s) => s.live)
  const rosterLen = useCockpit((s) => s.roster.length)
  const ledgerLen = useCockpit((s) => s.ledgerLen)
  const activeTab = useCockpit((s) => s.activeTab)
  const activeChannel = useCockpit((s) => s.activeChannel)
  const setTab = useCockpit((s) => s.setTab)
  const selectChannel = useCockpit((s) => s.selectChannel)
  const rooms = useRooms()
  const counts = useRoomCounts()

  const phaseTone =
    phase === CockpitPhase.Open
      ? 'bg-emerald-900 text-emerald-300'
      : phase === CockpitPhase.Paused
        ? 'bg-amber-900 text-amber-300'
        : 'bg-zinc-800 text-zinc-400'

  const status = statusText ?? (live ? (connected ? '● live' : '○ …') : 'no session')

  const openRoom = (key: string) => {
    selectChannel(key)
    setTab(CockpitTab.Chat)
  }

  return (
    <div className="flex h-full w-full flex-col bg-zinc-950 text-zinc-100">
      <div className="border-b border-zinc-800 p-3">
        <div className="flex items-center justify-between">
          <span className={`rounded-full px-2 py-0.5 text-[10px] uppercase tracking-wide ${phaseTone}`}>{phase}</span>
          <span className="text-xs text-zinc-500">{status}</span>
        </div>
        <div className="mt-2 flex gap-4 text-xs text-zinc-500">
          <span>
            <span className="text-zinc-200">{rosterLen}</span> guests
          </span>
          <span>
            <span className="text-zinc-200">{ledgerLen}</span> events
          </span>
        </div>
        {live && <PerspectiveSelect />}
      </div>

      <div className="px-3 py-2 text-[10px] uppercase tracking-widest text-zinc-500">Rooms</div>
      <div className="min-h-0 flex-1 overflow-auto pb-2">
        {rooms.length === 0 && <div className="px-3 py-2 text-xs text-zinc-600">Rooms appear once a session is live.</div>}
        {rooms.map((r) => {
          const active = activeTab === CockpitTab.Chat && r.key === activeChannel
          return (
            <button
              key={r.key}
              onClick={() => openRoom(r.key)}
              className={clsx(
                'flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm',
                active ? 'bg-zinc-800 text-zinc-100' : 'text-zinc-400 hover:bg-zinc-900/60',
              )}
            >
              <span className="shrink-0 text-xs">{channelGlyph(r.kind)}</span>
              <span className="min-w-0 flex-1 truncate">{r.title}</span>
              {counts.get(r.key) ? <span className="shrink-0 text-[10px] text-zinc-600">{counts.get(r.key)}</span> : null}
            </button>
          )
        })}
      </div>
    </div>
  )
}
