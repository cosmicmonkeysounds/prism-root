//! Operate mode — center stage: the live admin cockpit. Five pages over one
//! mod stream: Event (launch / codes / controls), Chat (post to the room picked
//! in the left rail), Roster (guests + cast), World (factions + locations, each
//! expandable + inspectable), and Director (fire signals / beats / broadcasts).
//! Owns the mod-stream lifecycle for the mode.

import { useEffect, useMemo, useState } from 'react'
import clsx from 'clsx'
import { useWorkspace } from '@/store/workspace'
import { useOperate, type RosterRow, type RunTab } from '@/store/operate'
import { StoryGraphPanel } from '@/components/graph/StoryGraphPanel'
import { FactionPill } from './ui'
import { useRooms } from './rooms'
import { useInspect } from './inspect'
import { EventPanel } from './EventPanel'

const TABS: { id: RunTab; label: string }[] = [
  { id: 'event', label: 'Event' },
  { id: 'chat', label: 'Chat' },
  { id: 'roster', label: 'Roster' },
  { id: 'world', label: 'World' },
  { id: 'story', label: 'Story' },
  { id: 'director', label: 'Director' },
]

// ---------------------------------------------------------------------------
// Chat — the selected room's thread + an operator composer (rooms live in the
// left rail; selecting one there focuses this page on it).
// ---------------------------------------------------------------------------

function ChatTab() {
  const messages = useOperate((s) => s.messages)
  const cast = useOperate((s) => s.cast)
  const active = useOperate((s) => s.activeChannel)
  const hideMessage = useOperate((s) => s.hideMessage)
  const say = useOperate((s) => s.say)
  const rooms = useRooms()

  const [text, setText] = useState('')
  const [asWho, setAsWho] = useState('') // '' → the room's default speaker

  const activeRoom = rooms.find((r) => r.key === active)
  const thread = useMemo(() => {
    const ch = activeRoom?.channel ?? active
    const g = activeRoom?.dmGuest ?? null
    return messages
      .filter((m) => (g ? m.channel === ch && Array.isArray(m.audience) && m.audience.includes(g) : m.channel === ch))
      .sort((a, b) => a.seq - b.seq)
  }, [messages, activeRoom, active])

  // Postable: any non-DM room, and any per-guest DM room (scoped safely via the
  // `guest:<id>` path). An aggregate DM room stays read-only.
  const canCompose = activeRoom ? (activeRoom.kind === 'dm' ? !!activeRoom.dmGuest : true) : !active.startsWith('dm:')
  const defaultSpeaker = activeRoom?.dmGuest && activeRoom.character ? activeRoom.character : 'Operator'

  const send = () => {
    const t = text.trim()
    if (!t || !canCompose) return
    if (activeRoom?.dmGuest) {
      void say(`guest:${activeRoom.dmGuest}`, t, asWho || activeRoom.character || undefined)
    } else {
      void say(activeRoom?.channel ?? active, t, asWho || undefined)
    }
    setText('')
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 border-b border-zinc-800 px-3 py-1.5 text-sm font-medium text-zinc-300">
        {activeRoom?.title ?? active}
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-2">
        {thread.length === 0 && <div className="p-2 text-sm text-zinc-600">No messages in this room yet.</div>}
        <ul className="flex flex-col gap-0.5">
          {thread.map((m) => (
            <li
              key={m.seq}
              className={clsx(
                'group flex items-start gap-2 rounded px-2 py-1 text-sm',
                m.hidden && 'opacity-40',
                m.parentSeq != null && 'ml-5 border-l border-zinc-800 pl-2',
              )}
            >
              <span className="shrink-0 font-medium text-zinc-300">{m.from || '·'}</span>
              <span className="min-w-0 flex-1 break-words text-zinc-200">{m.text}</span>
              <button
                onClick={() => void hideMessage(m.seq, !m.hidden)}
                className="shrink-0 rounded px-1.5 py-0.5 text-[10px] text-zinc-500 opacity-0 hover:bg-zinc-800 hover:text-zinc-200 group-hover:opacity-100"
              >
                {m.hidden ? 'show' : 'hide'}
              </button>
            </li>
          ))}
        </ul>
      </div>
      {canCompose ? (
        <div className="flex items-center gap-2 border-t border-zinc-800 p-2">
          <select
            value={asWho}
            onChange={(e) => setAsWho(e.target.value)}
            className="w-28 shrink-0 rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-xs text-zinc-200 outline-none focus:border-indigo-500"
            title="Post as"
          >
            <option value="">{defaultSpeaker}</option>
            {cast.map((c) => (
              <option key={c.id} value={c.id}>
                {c.id}
              </option>
            ))}
          </select>
          <input
            className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-sm outline-none focus:border-indigo-500"
            placeholder={`Message ${activeRoom?.title ?? active}…`}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && send()}
          />
          <button onClick={send} className="rounded bg-indigo-600 px-3 py-1 text-sm text-white hover:bg-indigo-500">
            Send
          </button>
        </div>
      ) : (
        <div className="border-t border-zinc-800 p-2 text-center text-xs text-zinc-600">
          This DM spans multiple guests — reply to one from their Inspector.
        </div>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Roster — guests table + the cast
// ---------------------------------------------------------------------------

function RosterTab() {
  const roster = useOperate((s) => s.roster)
  const cast = useOperate((s) => s.cast)
  const selection = useOperate((s) => s.selection)
  const capture = useOperate((s) => s.capture)
  const release = useOperate((s) => s.release)
  const inspect = useInspect()

  return (
    <div className="h-full overflow-auto">
      <table className="w-full text-sm">
        <thead className="sticky top-0 bg-zinc-950 text-left text-[10px] uppercase tracking-wide text-zinc-500">
          <tr className="border-b border-zinc-800">
            <th className="px-3 py-2">Guest</th>
            <th className="px-3 py-2">Faction</th>
            <th className="px-3 py-2">Location</th>
            <th className="px-3 py-2">Score</th>
            <th className="px-3 py-2"></th>
          </tr>
        </thead>
        <tbody>
          {roster.length === 0 && (
            <tr>
              <td colSpan={5} className="px-3 py-4 text-sm text-zinc-600">
                No participants yet.
              </td>
            </tr>
          )}
          {roster.map((r: RosterRow) => (
            <tr
              key={r.id}
              onClick={() => inspect({ kind: 'guest', id: r.id })}
              className={clsx(
                'cursor-pointer border-b border-zinc-900 hover:bg-zinc-900/60',
                selection?.kind === 'guest' && selection.id === r.id && 'bg-zinc-800/60',
              )}
            >
              <td className="px-3 py-2">
                {r.name} {r.captured && <span className="ml-1 text-xs text-red-400">🔒</span>}
                <div className="text-[10px] text-zinc-600">{r.id}</div>
              </td>
              <td className="px-3 py-2">
                <FactionPill faction={r.trueFaction ?? r.faction} />
              </td>
              <td className="px-3 py-2 text-zinc-400">{r.location ?? '—'}</td>
              <td className="px-3 py-2 text-zinc-300">{r.score}</td>
              <td className="px-3 py-2 text-right" onClick={(e) => e.stopPropagation()}>
                {r.captured ? (
                  <button onClick={() => void release(r.id)} className="rounded px-2 py-1 text-xs text-emerald-400 hover:bg-zinc-800">
                    release
                  </button>
                ) : (
                  <button onClick={() => void capture(r.id)} className="rounded px-2 py-1 text-xs text-amber-400 hover:bg-zinc-800">
                    capture
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      <div className="px-3 pt-4 pb-1 text-[10px] uppercase tracking-widest text-zinc-500">Cast</div>
      <div className="flex flex-wrap gap-1.5 px-3 pb-4">
        {cast.length === 0 && <div className="text-xs text-zinc-600">No characters authored.</div>}
        {cast.map((c) => (
          <button
            key={c.id}
            onClick={() => inspect({ kind: 'character', id: c.id })}
            className={clsx(
              'flex items-center gap-1.5 rounded-lg border px-2 py-1 text-xs',
              selection?.kind === 'character' && selection.id === c.id
                ? 'border-indigo-500/60 bg-zinc-800'
                : 'border-zinc-800 hover:bg-zinc-900/60',
            )}
          >
            <span className="text-zinc-200">{c.id}</span>
            <FactionPill faction={c.faction} />
          </button>
        ))}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// World — factions + locations (expandable + inspectable) + a stats footer
// ---------------------------------------------------------------------------

function Chevron({ open }: { open: boolean }) {
  return <span className="w-3 shrink-0 text-zinc-500">{open ? '▾' : '▸'}</span>
}

function WorldTab() {
  const factions = useOperate((s) => s.factions)
  const locations = useOperate((s) => s.locations)
  const roster = useOperate((s) => s.roster)
  const phase = useOperate((s) => s.phase)
  const scenario = useOperate((s) => s.scenario)
  const ledgerLen = useOperate((s) => s.ledgerLen)
  const rosterLen = useOperate((s) => s.roster.length)
  const selection = useOperate((s) => s.selection)
  const inspect = useInspect()

  const [open, setOpen] = useState<Set<string>>(new Set())
  const toggle = (k: string) =>
    setOpen((prev) => {
      const n = new Set(prev)
      if (n.has(k)) n.delete(k)
      else n.add(k)
      return n
    })
  const nameOf = (id: string) => roster.find((r) => r.id === id)?.name ?? id

  return (
    <div className="h-full overflow-auto p-3">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <div className="rounded-lg border border-zinc-800 p-2 text-center">
          <div className="text-lg font-semibold text-zinc-100">{rosterLen}</div>
          <div className="text-[10px] uppercase text-zinc-500">guests</div>
        </div>
        <div className="rounded-lg border border-zinc-800 p-2 text-center">
          <div className="text-lg font-semibold text-zinc-100">{ledgerLen}</div>
          <div className="text-[10px] uppercase text-zinc-500">events</div>
        </div>
        <div className="rounded-lg border border-zinc-800 p-2 text-center">
          <div className="text-lg font-semibold text-zinc-100">{phase}</div>
          <div className="text-[10px] uppercase text-zinc-500">phase</div>
        </div>
        <div className="truncate rounded-lg border border-zinc-800 p-2 text-center">
          <div className="truncate text-sm font-semibold text-zinc-100">{scenario ?? '—'}</div>
          <div className="text-[10px] uppercase text-zinc-500">scenario</div>
        </div>
      </div>

      <div className="pt-4 text-[10px] uppercase tracking-widest text-zinc-500">Factions</div>
      <div className="flex flex-col gap-1 pt-1">
        {factions.map((f) => {
          const k = `f:${f.id}`
          const selected = selection?.kind === 'faction' && selection.id === f.id
          return (
            <div key={f.id} className={clsx('rounded-lg border', selected ? 'border-indigo-500/50' : 'border-zinc-800')}>
              <div className="flex items-center gap-2 px-2 py-1.5 text-sm">
                <button onClick={() => toggle(k)} title="Toggle detail" className="hover:text-zinc-200">
                  <Chevron open={open.has(k)} />
                </button>
                <button onClick={() => inspect({ kind: 'faction', id: f.id })} className="flex min-w-0 flex-1 items-center gap-2 text-left">
                  <FactionPill faction={f.id} />
                  <span className="text-zinc-400">{f.members.length} members</span>
                  {f.hidden && (
                    <span className={f.revealed ? 'text-red-400' : 'text-zinc-600'}>{f.revealed ? '· exposed' : '· hidden'}</span>
                  )}
                </button>
              </div>
              {open.has(k) && (
                <div className="space-y-0.5 border-t border-zinc-800 px-3 py-2 text-xs text-zinc-400">
                  {f.ethos && <div>Ethos: <span className="text-zinc-300">{f.ethos}</span></div>}
                  {f.rival && <div>Rival: <span className="text-zinc-300">{f.rival}</span></div>}
                  <div>Members: {f.members.length ? f.members.map(nameOf).join(', ') : '—'}</div>
                </div>
              )}
            </div>
          )
        })}
      </div>

      <div className="pt-4 text-[10px] uppercase tracking-widest text-zinc-500">Locations</div>
      <div className="flex flex-col gap-1 pt-1">
        {locations.map((l) => {
          const k = `l:${l.id}`
          const selected = selection?.kind === 'location' && selection.id === l.id
          return (
            <div key={l.id} className={clsx('rounded-lg border', selected ? 'border-indigo-500/50' : 'border-zinc-800')}>
              <div className="flex items-center gap-2 px-2 py-1.5 text-sm">
                <button onClick={() => toggle(k)} title="Toggle detail" className="hover:text-zinc-200">
                  <Chevron open={open.has(k)} />
                </button>
                <button onClick={() => inspect({ kind: 'location', id: l.id })} className="flex min-w-0 flex-1 items-center gap-2 text-left">
                  <span className="truncate text-zinc-200">{l.label || l.id}</span>
                  {l.prison && <span className="text-xs text-red-400">prison</span>}
                  <span className="ml-auto text-zinc-400">{l.occupants.length} here</span>
                </button>
              </div>
              {open.has(k) && (
                <div className="border-t border-zinc-800 px-3 py-2 text-xs text-zinc-400">
                  Occupants: {l.occupants.length ? l.occupants.map(nameOf).join(', ') : '—'}
                </div>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Director — fire signals / beats / broadcasts
// ---------------------------------------------------------------------------

function SubjectSelect({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const roster = useOperate((s) => s.roster)
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="rounded border border-zinc-700 bg-zinc-950 px-2 py-1.5 text-sm text-zinc-200 outline-none focus:border-indigo-500"
    >
      <option value="">— everyone —</option>
      {roster.map((r) => (
        <option key={r.id} value={r.id}>
          {r.name} ({r.id})
        </option>
      ))}
    </select>
  )
}

function DirectorTab() {
  const beats = useOperate((s) => s.beats)
  const fireSignal = useOperate((s) => s.fireSignal)
  const fireBeat = useOperate((s) => s.fireBeat)
  const broadcast = useOperate((s) => s.broadcast)

  const [signal, setSignal] = useState('')
  const [signalSubject, setSignalSubject] = useState('')
  const [beat, setBeat] = useState('')
  const [beatSubject, setBeatSubject] = useState('')
  const [cue, setCue] = useState('')
  const [scope, setScope] = useState('all')

  return (
    <div className="h-full overflow-auto p-3">
      <div className="mx-auto flex max-w-lg flex-col gap-4">
        <section className="rounded-xl border border-zinc-800 p-3">
          <div className="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-400">Fire signal</div>
          <p className="mb-2 text-xs text-zinc-600">Triggers every <code>on &lt;name&gt;</code> hook — globally or on one guest.</p>
          <div className="flex flex-wrap gap-2">
            <input
              value={signal}
              onChange={(e) => setSignal(e.target.value)}
              placeholder="signal name (e.g. lockdown)"
              className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1.5 text-sm outline-none focus:border-indigo-500"
            />
            <SubjectSelect value={signalSubject} onChange={setSignalSubject} />
            <button
              onClick={() => signal.trim() && void fireSignal(signal.trim(), signalSubject || undefined)}
              className="rounded bg-indigo-600 px-3 py-1.5 text-sm text-white hover:bg-indigo-500"
            >
              Fire
            </button>
          </div>
        </section>

        <section className="rounded-xl border border-zinc-800 p-3">
          <div className="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-400">Fire beat</div>
          <p className="mb-2 text-xs text-zinc-600">Inject a named story beat, optionally targeting one guest.</p>
          <div className="flex flex-wrap gap-2">
            <select
              value={beat}
              onChange={(e) => setBeat(e.target.value)}
              className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1.5 text-sm text-zinc-200 outline-none focus:border-indigo-500"
            >
              <option value="">— select a beat —</option>
              {beats.map((b) => (
                <option key={b} value={b}>
                  {b}
                </option>
              ))}
            </select>
            <SubjectSelect value={beatSubject} onChange={setBeatSubject} />
            <button
              onClick={() => beat && void fireBeat(beat, beatSubject || undefined)}
              disabled={!beat}
              className="rounded bg-indigo-600 px-3 py-1.5 text-sm text-white hover:bg-indigo-500 disabled:opacity-40"
            >
              Fire
            </button>
          </div>
        </section>

        <section className="rounded-xl border border-zinc-800 p-3">
          <div className="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-400">Broadcast cue</div>
          <p className="mb-2 text-xs text-zinc-600">A room-wide cue, scoped (e.g. <code>faction(Mods)</code>, <code>location(Internet)</code>).</p>
          <div className="flex flex-wrap gap-2">
            <input
              value={cue}
              onChange={(e) => setCue(e.target.value)}
              placeholder="cue (e.g. intermission)"
              className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1.5 text-sm outline-none focus:border-indigo-500"
            />
            <input
              value={scope}
              onChange={(e) => setScope(e.target.value)}
              placeholder="scope"
              className="w-32 rounded border border-zinc-700 bg-zinc-950 px-2 py-1.5 text-sm outline-none focus:border-indigo-500"
            />
            <button
              onClick={() => cue.trim() && void broadcast(scope.trim() || 'all', cue.trim())}
              className="rounded bg-indigo-600 px-3 py-1.5 text-sm text-white hover:bg-indigo-500"
            >
              Send
            </button>
          </div>
        </section>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

export function OperateStage() {
  const projectId = useWorkspace((s) => s.projectId)
  const init = useOperate((s) => s.init)
  const teardown = useOperate((s) => s.teardown)
  const event = useOperate((s) => s.event)
  const connected = useOperate((s) => s.connected)
  const tab = useOperate((s) => s.activeTab)
  const setTab = useOperate((s) => s.setTab)

  useEffect(() => {
    if (!projectId) return
    void init(projectId)
    return () => teardown()
  }, [projectId, init, teardown])

  if (!projectId) {
    return (
      <div className="grid h-full w-full place-items-center bg-zinc-950 p-8 text-center text-sm text-zinc-500">
        Open a server project from the launchpad to run events.
        <br />
        (Local folders can be authored, but events run on server projects.)
      </div>
    )
  }

  return (
    <div className="flex h-full w-full flex-col bg-zinc-950 text-zinc-100">
      <div className="flex items-center justify-between border-b border-zinc-800 px-2">
        <div role="tablist" className="flex items-stretch">
          {TABS.map((t) => (
            <button
              key={t.id}
              role="tab"
              aria-selected={tab === t.id}
              onClick={() => setTab(t.id)}
              className={clsx(
                'border-b-2 px-3 py-2 text-sm transition-colors',
                tab === t.id ? 'border-indigo-400 text-zinc-100' : 'border-transparent text-zinc-500 hover:text-zinc-200',
              )}
            >
              {t.label}
            </button>
          ))}
        </div>
        <span className="pr-1 text-xs text-zinc-500">
          {event ? (connected ? '● live' : '○ reconnecting…') : 'no active event'}
        </span>
      </div>
      <div className="min-h-0 flex-1">
        {tab === 'event' && <EventPanel />}
        {tab === 'chat' && <ChatTab />}
        {tab === 'roster' && <RosterTab />}
        {tab === 'world' && <WorldTab />}
        {tab === 'story' && <StoryGraphPanel variant="run" />}
        {tab === 'director' && <DirectorTab />}
      </div>
    </div>
  )
}
