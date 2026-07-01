//! Operate mode — left rail: a persistent event-status strip over the Rooms
//! navigator. Clicking a room jumps to the Chat page focused on it, so the
//! channel list is always one click away regardless of the active page.

import clsx from 'clsx'
import { useOperate } from '@/store/operate'
import { useRooms, useRoomCounts } from './rooms'
import { channelGlyph } from './format'

export function RoomsRail() {
  const event = useOperate((s) => s.event)
  const phase = useOperate((s) => s.phase)
  const connected = useOperate((s) => s.connected)
  const rosterLen = useOperate((s) => s.roster.length)
  const ledgerLen = useOperate((s) => s.ledgerLen)
  const activeTab = useOperate((s) => s.activeTab)
  const activeChannel = useOperate((s) => s.activeChannel)
  const setTab = useOperate((s) => s.setTab)
  const selectChannel = useOperate((s) => s.selectChannel)
  const rooms = useRooms()
  const counts = useRoomCounts()

  const phaseTone =
    phase === 'open'
      ? 'bg-emerald-900 text-emerald-300'
      : phase === 'paused'
        ? 'bg-amber-900 text-amber-300'
        : 'bg-zinc-800 text-zinc-400'

  const openRoom = (key: string) => {
    selectChannel(key)
    setTab('chat')
  }

  return (
    <div className="flex h-full w-full flex-col bg-zinc-950 text-zinc-100">
      <div className="border-b border-zinc-800 p-3">
        <div className="flex items-center justify-between">
          <span className={`rounded-full px-2 py-0.5 text-[10px] uppercase tracking-wide ${phaseTone}`}>{phase}</span>
          <span className="text-xs text-zinc-500">{event ? (connected ? '● live' : '○ …') : 'no event'}</span>
        </div>
        <div className="mt-2 flex gap-4 text-xs text-zinc-500">
          <span>
            <span className="text-zinc-200">{rosterLen}</span> guests
          </span>
          <span>
            <span className="text-zinc-200">{ledgerLen}</span> events
          </span>
        </div>
      </div>

      <div className="px-3 py-2 text-[10px] uppercase tracking-widest text-zinc-500">Rooms</div>
      <div className="min-h-0 flex-1 overflow-auto pb-2">
        {rooms.length === 0 && <div className="px-3 py-2 text-xs text-zinc-600">Rooms appear once the event is live.</div>}
        {rooms.map((r) => {
          const active = activeTab === 'chat' && r.key === activeChannel
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
