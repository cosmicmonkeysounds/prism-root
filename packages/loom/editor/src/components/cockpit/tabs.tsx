//! The shared cockpit center pages — Chat / Roster / World / Director —
//! rendered identically on Run mode's Sim (local simulator) and Live
//! (event) sources. Everything reads through `useCockpit`, so the enclosing
//! provider decides which backend the page drives.

import { useMemo, useState } from 'react'
import clsx from 'clsx'
import {
  CockpitTab,
  SelectionKind,
  useCockpit,
  type CockpitMessage,
  type RosterRow,
  type Selection,
} from '@/store/cockpit'
import { useGraph } from '@/store/graph'
import { openContextMenu, type ContextMenuEntry } from '@/store/context-menu'
import { FactionPill } from './ui'
import { LensKind, lensKindOf, messageInLens, useRooms } from './rooms'
import { useInspect } from './inspect'

// ---------------------------------------------------------------------------
// Chat — the selected room's thread + an act-as-anyone composer (rooms live in
// the left rail; selecting one there focuses this page on it). Messages render
// by kind: scripted `line`s group by sender like the play app, `narration` is
// the Narrator's stage voice, `system`/`signal` are notices.
// ---------------------------------------------------------------------------

/** A message's beat link — jumps to the node on the Story tab's map. */
function BeatLink({ beat }: { beat: string }) {
  const setTab = useCockpit((s) => s.setTab)
  return (
    <button
      onClick={() => {
        setTab(CockpitTab.Story)
        useGraph.getState().reveal(beat)
      }}
      title={`Show ${beat} on the story map`}
      className="shrink-0 rounded px-1 py-0.5 text-[10px] text-zinc-600 opacity-0 hover:bg-zinc-800 hover:text-amber-300 group-hover:opacity-100"
    >
      ⤷ {beat}
    </button>
  )
}

function MessageRow({
  m,
  banner,
  onHide,
  onMenu,
}: {
  m: CockpitMessage
  banner: boolean
  onHide: () => void
  onMenu?: (e: React.MouseEvent) => void
}) {
  const hideBtn = (
    <button
      onClick={onHide}
      className="shrink-0 rounded px-1.5 py-0.5 text-[10px] text-zinc-500 opacity-0 hover:bg-zinc-800 hover:text-zinc-200 group-hover:opacity-100"
    >
      {m.hidden ? 'show' : 'hide'}
    </button>
  )
  const threadIndent = m.parentSeq != null && 'ml-5 border-l border-zinc-800 pl-2'

  if (m.kind === 'narration') {
    return (
      <li onContextMenu={onMenu} className={clsx('group flex items-start gap-2 rounded bg-zinc-900/50 px-3 py-1.5', m.hidden && 'opacity-40', threadIndent)}>
        <span className="shrink-0 pt-px text-[10px] uppercase tracking-widest text-amber-500/80">{m.from || 'Narrator'}</span>
        <span className="min-w-0 flex-1 break-words text-sm italic text-zinc-300">{m.text}</span>
        {m.beat ? <BeatLink beat={m.beat} /> : null}
        {hideBtn}
      </li>
    )
  }
  if (m.kind === 'system') {
    return (
      <li onContextMenu={onMenu} className={clsx('group flex items-center gap-2 px-3 py-0.5', m.hidden && 'opacity-40', threadIndent)}>
        <span className="min-w-0 flex-1 break-words text-center text-xs text-zinc-500">{m.text}</span>
        {hideBtn}
      </li>
    )
  }
  if (m.kind === 'signal') {
    return (
      <li onContextMenu={onMenu} className={clsx('group flex items-start gap-2 rounded px-2 py-1', m.hidden && 'opacity-40', threadIndent)}>
        <span className="min-w-0 flex-1 break-words text-sm text-amber-200/90">{m.text}</span>
        {hideBtn}
      </li>
    )
  }
  // A spoken `line` — the sender banner appears once per consecutive run.
  return (
    <li onContextMenu={onMenu} className={clsx('group flex flex-col rounded px-2', banner ? 'pt-1.5' : 'pt-0', 'pb-0.5', m.hidden && 'opacity-40', threadIndent)}>
      {banner && <span className="text-xs font-semibold text-zinc-300">{m.from || '·'}</span>}
      <span className="flex items-start gap-2">
        <span className="min-w-0 flex-1 break-words text-sm text-zinc-200">{m.text}</span>
        {m.beat ? <BeatLink beat={m.beat} /> : null}
        {hideBtn}
      </span>
    </li>
  )
}

/** The lens persona's pending choice, docked in the room above the composer. */
function DecisionTray({ person, options }: { person: string; options: string[] }) {
  const choose = useCockpit((s) => s.choose)
  const roster = useCockpit((s) => s.roster)
  const name = person === '__global' ? 'the story' : (roster.find((r) => r.id === person)?.name ?? person)
  return (
    <div className="border-t border-indigo-900/60 bg-indigo-950/30 p-2">
      <div className="pb-1.5 text-[10px] uppercase tracking-widest text-indigo-300/80">Decision — {name}</div>
      <div className="flex flex-wrap gap-1.5">
        {options.map((opt, i) => (
          <button
            key={`${i}-${opt}`}
            onClick={() => void choose(person, i)}
            className="rounded border border-indigo-500/50 bg-indigo-600/20 px-2.5 py-1 text-sm text-indigo-100 hover:bg-indigo-600/40"
          >
            {opt}
          </button>
        ))}
      </div>
    </div>
  )
}

export function ChatTab() {
  const messages = useCockpit((s) => s.messages)
  const cast = useCockpit((s) => s.cast)
  const roster = useCockpit((s) => s.roster)
  const locations = useCockpit((s) => s.locations)
  const choices = useCockpit((s) => s.choices)
  const active = useCockpit((s) => s.activeChannel)
  const perspective = useCockpit((s) => s.perspective)
  const hideMessage = useCockpit((s) => s.hideMessage)
  const say = useCockpit((s) => s.say)
  const setPerspective = useCockpit((s) => s.setPerspective)
  const setTab = useCockpit((s) => s.setTab)
  const inspect = useInspect()
  const rooms = useRooms()

  const [text, setText] = useState('')
  const [asWho, setAsWho] = useState('') // '' → the lens/room default speaker
  /** The message a composed reply threads under (root of its thread). */
  const [replyTo, setReplyTo] = useState<CockpitMessage | null>(null)
  const [replyRoom, setReplyRoom] = useState(active)
  // A reply targets a message in THIS room — leaving the room drops it
  // (the adjust-state-during-render pattern, not an effect).
  if (active !== replyRoom) {
    setReplyRoom(active)
    setReplyTo(null)
  }

  const lens = lensKindOf(perspective, roster, cast)
  const activeRoom = rooms.find((r) => r.key === active)
  const thread = useMemo(() => {
    const ch = activeRoom?.channel ?? active
    const g = activeRoom?.dmGuest ?? null
    return messages
      .filter((m) => (g ? m.channel === ch && Array.isArray(m.audience) && m.audience.includes(g) : m.channel === ch))
      .filter((m) => messageInLens(m, perspective, lens))
      .sort((a, b) => a.seq - b.seq)
  }, [messages, activeRoom, active, perspective, lens])

  // Postable: any non-DM room, and any per-guest DM room (scoped safely via the
  // `guest:<id>` path). An aggregate DM room stays read-only.
  const canCompose = activeRoom ? (activeRoom.kind === 'dm' ? !!activeRoom.dmGuest : true) : !active.startsWith('dm:')

  // The default voice: the lens persona when one is active, the DM room's
  // character in a guest thread, else the Operator.
  const defaultSpeaker =
    lens === LensKind.Guest || lens === LensKind.Performer
      ? perspective
      : activeRoom?.dmGuest && activeRoom.character
        ? activeRoom.character
        : 'Operator'
  const speaker = asWho || defaultSpeaker
  const speakerIsGuest = roster.some((r) => r.id === speaker)
  const speakerName = speakerIsGuest ? (roster.find((r) => r.id === speaker)?.name ?? speaker) : speaker

  // A guest can only speak in a location room they're standing in (the
  // engine's canPost rule); Operator/Narrator/cast voices are stage crew.
  const notPresent =
    speakerIsGuest && activeRoom?.kind === 'location'
      ? !(locations.find((l) => `loc:${l.id}` === activeRoom.channel)?.occupants.includes(speaker) ?? false)
      : false

  const send = () => {
    const t = text.trim()
    if (!t || !canCompose || notPresent) return
    // Slack-style: replies root at the thread parent, not the reply itself.
    const parent = replyTo !== null ? (replyTo.parentSeq ?? replyTo.seq) : null
    if (activeRoom?.dmGuest && speaker !== activeRoom.dmGuest) {
      // Speaking to the guest in their thread, as the character/Operator.
      void say(`guest:${activeRoom.dmGuest}`, t, asWho || activeRoom.character || undefined, parent)
    } else {
      // Speaking in the room as whoever the lens/picker says — a guest voice
      // goes through the same journaled say path the play app uses.
      void say(activeRoom?.channel ?? active, t, speaker, parent)
    }
    setText('')
    setReplyTo(null)
  }

  /** Who a message's `from` resolves to (guest by name/id, cast by id). */
  const senderOf = (m: CockpitMessage): Selection | null => {
    const g = roster.find((r) => r.name === m.from || r.id === m.from)
    if (g !== undefined) return { kind: SelectionKind.Guest, id: g.id }
    const c = cast.find((c) => c.id === m.from)
    if (c !== undefined) return { kind: SelectionKind.Character, id: c.id }
    return null
  }

  /** The message context menu — copy / reply / map / sender / moderation. */
  const messageMenu = (m: CockpitMessage, e: React.MouseEvent) => {
    e.preventDefault()
    const items: ContextMenuEntry[] = [
      { label: 'Copy text', onSelect: () => void navigator.clipboard?.writeText(m.text) },
    ]
    if (canCompose) {
      items.push({
        label: 'Reply in thread',
        testid: 'chat-menu-reply',
        onSelect: () => setReplyTo(m),
      })
    }
    if (m.beat) {
      items.push({
        label: `Show ${m.beat} on story map`,
        onSelect: () => {
          setTab(CockpitTab.Story)
          useGraph.getState().reveal(m.beat!)
        },
      })
    }
    const sender = senderOf(m)
    if (sender !== null) {
      items.push(
        { separator: true },
        { label: `Inspect ${m.from}`, testid: 'chat-menu-inspect', onSelect: () => inspect(sender) },
        { label: `View as ${m.from}`, onSelect: () => setPerspective(sender.id) },
      )
    }
    items.push(
      { separator: true },
      {
        label: m.hidden ? 'Show message' : 'Hide message',
        testid: 'chat-menu-hide',
        onSelect: () => void hideMessage(m.seq, !m.hidden),
      },
    )
    openContextMenu(items, { x: e.clientX, y: e.clientY })
  }

  // The lens persona's pending decision docks here; the operator lens also
  // surfaces unbound (global) story menus.
  const decisionPerson =
    lens === LensKind.Guest && choices[perspective] ? perspective
    : lens === LensKind.Operator && choices['__global'] ? '__global'
    : null

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-zinc-800 px-3 py-1.5">
        <span className="text-sm font-medium text-zinc-300">{activeRoom?.title ?? active}</span>
        {activeRoom && <span className="text-[10px] uppercase tracking-wide text-zinc-600">{activeRoom.kind}</span>}
        {lens !== LensKind.Operator && (
          <span className="ml-auto rounded-full bg-indigo-950 px-2 py-0.5 text-[10px] text-indigo-300">
            viewing as {roster.find((r) => r.id === perspective)?.name ?? perspective}
          </span>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-2">
        {thread.length === 0 && <div className="p-2 text-sm text-zinc-600">No messages in this room yet.</div>}
        <ul className="flex flex-col gap-0.5">
          {thread.map((m, i) => {
            const prev = thread[i - 1]
            const banner = m.kind !== 'line' || prev === undefined || prev.kind !== 'line' || prev.from !== m.from
            return (
              <MessageRow
                key={m.seq}
                m={m}
                banner={banner}
                onHide={() => void hideMessage(m.seq, !m.hidden)}
                onMenu={(e) => messageMenu(m, e)}
              />
            )
          })}
        </ul>
      </div>
      {decisionPerson && choices[decisionPerson] ? (
        <DecisionTray person={decisionPerson} options={choices[decisionPerson]!} />
      ) : null}
      {canCompose ? (
        <div className="flex flex-col border-t border-zinc-800">
          {notPresent && (
            <div className="px-3 pt-1.5 text-[10px] text-amber-400/80">
              {speakerName} isn't in this room — move them here first, or speak as the Operator.
            </div>
          )}
          {replyTo !== null && (
            <div className="flex items-center gap-2 px-3 pt-1.5 text-[11px] text-indigo-300" data-testid="chat-reply-chip">
              <span className="min-w-0 truncate">
                ↩ Replying to {replyTo.from || 'Narrator'}: “{replyTo.text.slice(0, 80)}”
              </span>
              <button
                onClick={() => setReplyTo(null)}
                className="shrink-0 rounded px-1 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200"
                aria-label="Cancel reply"
              >
                ✕
              </button>
            </div>
          )}
          <div className="flex items-center gap-2 p-2">
            <select
              value={asWho}
              onChange={(e) => setAsWho(e.target.value)}
              className="w-32 shrink-0 rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-xs text-zinc-200 outline-none focus:border-indigo-500"
              title="Post as"
              data-testid="chat-post-as"
            >
              <option value="">{speakerIsGuest && asWho === '' ? speakerName : defaultSpeaker}</option>
              <optgroup label="Story">
                <option value="Operator">Operator</option>
                <option value="Narrator">Narrator</option>
              </optgroup>
              {roster.length > 0 && (
                <optgroup label="Guests">
                  {roster.map((r) => (
                    <option key={r.id} value={r.id}>
                      {r.name}
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
            <input
              className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-sm outline-none focus:border-indigo-500"
              placeholder={`Message ${activeRoom?.title ?? active} as ${speakerName}…`}
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && send()}
            />
            <button
              onClick={send}
              disabled={notPresent}
              className="rounded bg-indigo-600 px-3 py-1 text-sm text-white hover:bg-indigo-500 disabled:opacity-40"
            >
              Send
            </button>
          </div>
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

export function RosterTab() {
  const roster = useCockpit((s) => s.roster)
  const cast = useCockpit((s) => s.cast)
  const selection = useCockpit((s) => s.selection)
  const capture = useCockpit((s) => s.capture)
  const release = useCockpit((s) => s.release)
  const setPerspective = useCockpit((s) => s.setPerspective)
  const inspect = useInspect()

  const guestMenu = (r: RosterRow, e: React.MouseEvent) => {
    e.preventDefault()
    openContextMenu(
      [
        { label: `Inspect ${r.name}`, onSelect: () => inspect({ kind: SelectionKind.Guest, id: r.id }) },
        { label: `View as ${r.name}`, onSelect: () => setPerspective(r.id) },
        { separator: true },
        r.captured
          ? { label: 'Release', onSelect: () => void release(r.id) }
          : { label: 'Capture', kind: 'danger' as const, onSelect: () => void capture(r.id) },
      ],
      { x: e.clientX, y: e.clientY },
    )
  }

  const castMenu = (id: string, e: React.MouseEvent) => {
    e.preventDefault()
    openContextMenu(
      [
        { label: `Inspect ${id}`, onSelect: () => inspect({ kind: SelectionKind.Character, id }) },
        { label: `View as ${id}`, onSelect: () => setPerspective(id) },
      ],
      { x: e.clientX, y: e.clientY },
    )
  }

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
              onClick={() => inspect({ kind: SelectionKind.Guest, id: r.id })}
              onContextMenu={(e) => guestMenu(r, e)}
              className={clsx(
                'cursor-pointer border-b border-zinc-900 hover:bg-zinc-900/60',
                selection?.kind === SelectionKind.Guest && selection.id === r.id && 'bg-zinc-800/60',
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
            onClick={() => inspect({ kind: SelectionKind.Character, id: c.id })}
            onContextMenu={(e) => castMenu(c.id, e)}
            className={clsx(
              'flex items-center gap-1.5 rounded-lg border px-2 py-1 text-xs',
              selection?.kind === SelectionKind.Character && selection.id === c.id
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

export function WorldTab() {
  const factions = useCockpit((s) => s.factions)
  const locations = useCockpit((s) => s.locations)
  const roster = useCockpit((s) => s.roster)
  const phase = useCockpit((s) => s.phase)
  const scenario = useCockpit((s) => s.scenario)
  const ledgerLen = useCockpit((s) => s.ledgerLen)
  const rosterLen = useCockpit((s) => s.roster.length)
  const selection = useCockpit((s) => s.selection)
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
          const selected = selection?.kind === SelectionKind.Faction && selection.id === f.id
          return (
            <div key={f.id} className={clsx('rounded-lg border', selected ? 'border-indigo-500/50' : 'border-zinc-800')}>
              <div className="flex items-center gap-2 px-2 py-1.5 text-sm">
                <button onClick={() => toggle(k)} title="Toggle detail" className="hover:text-zinc-200">
                  <Chevron open={open.has(k)} />
                </button>
                <button onClick={() => inspect({ kind: SelectionKind.Faction, id: f.id })} className="flex min-w-0 flex-1 items-center gap-2 text-left">
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
          const selected = selection?.kind === SelectionKind.Location && selection.id === l.id
          return (
            <div key={l.id} className={clsx('rounded-lg border', selected ? 'border-indigo-500/50' : 'border-zinc-800')}>
              <div className="flex items-center gap-2 px-2 py-1.5 text-sm">
                <button onClick={() => toggle(k)} title="Toggle detail" className="hover:text-zinc-200">
                  <Chevron open={open.has(k)} />
                </button>
                <button onClick={() => inspect({ kind: SelectionKind.Location, id: l.id })} className="flex min-w-0 flex-1 items-center gap-2 text-left">
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
// Director — fire named events / beats / broadcasts
// ---------------------------------------------------------------------------

export function SubjectSelect({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const roster = useCockpit((s) => s.roster)
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

/** Sentinel `<option>` that reveals the free-text signal input. */
const CUSTOM_EVENT = '__custom'

export function DirectorTab() {
  const beats = useCockpit((s) => s.beats)
  const events = useCockpit((s) => s.events)
  const fireSignal = useCockpit((s) => s.fireSignal)
  const fireBeat = useCockpit((s) => s.fireBeat)
  const broadcast = useCockpit((s) => s.broadcast)

  const [signal, setSignal] = useState('')
  const [custom, setCustom] = useState('')
  const [signalSubject, setSignalSubject] = useState('')
  const [beat, setBeat] = useState('')
  const [beatSubject, setBeatSubject] = useState('')
  const [cue, setCue] = useState('')
  const [scope, setScope] = useState('all')

  const chosenSignal = signal === CUSTOM_EVENT ? custom.trim() : signal

  return (
    <div className="h-full overflow-auto p-3">
      <div className="mx-auto flex max-w-lg flex-col gap-4">
        <section className="rounded-xl border border-zinc-800 p-3">
          <div className="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-400">Fire named event</div>
          <p className="mb-2 text-xs text-zinc-600">
            Triggers every <code>on &lt;name&gt;</code> hook — globally or on one guest. The list is
            every event the story declares.
          </p>
          <div className="flex flex-wrap gap-2">
            <select
              value={signal}
              onChange={(e) => setSignal(e.target.value)}
              className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1.5 text-sm text-zinc-200 outline-none focus:border-indigo-500"
              data-testid="director-event-select"
            >
              <option value="">— select an event —</option>
              {events.map((ev) => (
                <option key={ev} value={ev}>
                  {ev}
                </option>
              ))}
              <option value={CUSTOM_EVENT}>custom…</option>
            </select>
            {signal === CUSTOM_EVENT && (
              <input
                value={custom}
                onChange={(e) => setCustom(e.target.value)}
                placeholder="event name (e.g. lockdown)"
                className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1.5 text-sm outline-none focus:border-indigo-500"
              />
            )}
            <SubjectSelect value={signalSubject} onChange={setSignalSubject} />
            <button
              onClick={() => chosenSignal && void fireSignal(chosenSignal, signalSubject || undefined)}
              disabled={!chosenSignal}
              className="rounded bg-indigo-600 px-3 py-1.5 text-sm text-white hover:bg-indigo-500 disabled:opacity-40"
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
