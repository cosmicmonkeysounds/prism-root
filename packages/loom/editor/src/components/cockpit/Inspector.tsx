//! The cockpit's right tray: the Inspector. Bound to the Roster/World
//! selection, it's where the operator (or the writer, in Sim mode)
//! changes a guest's stats on the fly (score / faction / location /
//! captured), fires narrative at one person, answers a persona's
//! pending choice (Sim), or — for a cast member — fires that
//! character's owned beats.

import { useState, type ReactNode } from 'react'
import { CockpitTab, SelectionKind, useCockpit } from '@/store/cockpit'
import { FactionPill } from './ui'

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex items-center gap-2 px-3 py-1.5 text-xs">
      <span className="w-20 shrink-0 text-zinc-500">{label}</span>
      {children}
    </label>
  )
}

const inputCls =
  'min-w-0 flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-sm text-zinc-100 outline-none focus:border-indigo-500'

// ---------------------------------------------------------------------------
// Pending choice (Sim mode — the writer acts as the persona)
// ---------------------------------------------------------------------------

export function PendingChoice({ person }: { person: string }) {
  const options = useCockpit((s) => s.choices[person])
  const choose = useCockpit((s) => s.choose)
  if (options === undefined || options.length === 0) return null
  return (
    <div className="mx-3 mb-2 rounded-lg border border-emerald-700/50 bg-emerald-950/30 p-2">
      <div className="pb-1 text-[10px] uppercase tracking-widest text-emerald-400">Pending choice</div>
      <div className="flex flex-col gap-1">
        {options.map((opt, i) => (
          <button
            key={i}
            onClick={() => void choose(person, i)}
            className="rounded border border-emerald-800/60 px-2 py-1 text-left text-xs text-emerald-100 hover:bg-emerald-900/40"
            data-testid={`choice-${person}-${i}`}
          >
            {i + 1}. {opt}
          </button>
        ))}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Guest inspector
// ---------------------------------------------------------------------------

function GuestInspector({ id }: { id: string }) {
  const row = useCockpit((s) => s.roster.find((r) => r.id === id))
  const factions = useCockpit((s) => s.factions)
  const locations = useCockpit((s) => s.locations)
  const cast = useCockpit((s) => s.cast)
  const events = useCockpit((s) => s.events)
  const setStat = useCockpit((s) => s.setStat)
  const fireSignal = useCockpit((s) => s.fireSignal)
  const scanAs = useCockpit((s) => s.scanAs)
  const say = useCockpit((s) => s.say)
  const setPerspective = useCockpit((s) => s.setPerspective)
  const setTab = useCockpit((s) => s.setTab)

  const [score, setScore] = useState('')
  const [scoreSeen, setScoreSeen] = useState<number | null>(null)
  const [editing, setEditing] = useState(false)
  const [signal, setSignal] = useState('')
  const [scanChar, setScanChar] = useState('')

  if (!row) return <Empty msg="This guest has left the event." />

  // Resync the field to the live value only when the operator isn't mid-edit,
  // so an incoming snapshot (a tick, another mod's action) can't clobber a
  // half-typed score.
  if (!editing && row.score !== scoreSeen) {
    setScoreSeen(row.score)
    setScore(String(row.score))
  }

  const commitScore = () => {
    setEditing(false)
    setScoreSeen(row.score)
    const n = Number(score)
    if (Number.isFinite(n) && n !== row.score) void setStat(id, 'score', n)
  }

  const dm = () => {
    const text = window.prompt(`Message to ${row.name} (as Operator):`)?.trim()
    if (text) void say(`guest:${id}`, text)
  }

  return (
    <div className="h-full overflow-auto">
      <div className="border-b border-zinc-800 px-3 py-2">
        <div className="text-[10px] uppercase tracking-widest text-zinc-500">Guest</div>
        <div className="flex items-center gap-2">
          <span className="text-sm font-semibold text-zinc-100">{row.name}</span>
          {row.captured && <span className="text-xs text-red-400">🔒 captured</span>}
        </div>
        <div className="text-[10px] text-zinc-600">{row.id}</div>
      </div>

      <div className="pt-2">
        <PendingChoice person={id} />
      </div>

      <div className="py-1">
        <Field label="Score">
          <input
            className={inputCls}
            value={score}
            inputMode="numeric"
            onFocus={() => setEditing(true)}
            onChange={(e) => {
              setEditing(true)
              setScore(e.target.value)
            }}
            onBlur={commitScore}
            onKeyDown={(e) => {
              if (e.key === 'Enter') e.currentTarget.blur()
              else if (e.key === 'Escape') {
                setEditing(false)
                setScore(String(row.score))
              }
            }}
          />
        </Field>

        <Field label="Faction">
          <select
            className={inputCls}
            value={row.faction ?? ''}
            onChange={(e) => e.target.value && void setStat(id, 'faction', e.target.value)}
          >
            <option value="" disabled>
              — set faction —
            </option>
            {factions.map((f) => (
              <option key={f.id} value={f.id}>
                {f.id}
              </option>
            ))}
          </select>
        </Field>

        <Field label="Location">
          <select
            className={inputCls}
            value={row.location ?? ''}
            onChange={(e) => e.target.value && void setStat(id, 'location', e.target.value)}
          >
            <option value="" disabled>
              — set location —
            </option>
            {locations.map((l) => (
              <option key={l.id} value={l.id}>
                {l.label || l.id}
              </option>
            ))}
          </select>
        </Field>

        <Field label="Captured">
          <button
            onClick={() => void setStat(id, 'captured', !row.captured)}
            className={
              row.captured
                ? 'rounded bg-emerald-700/40 px-3 py-1 text-xs text-emerald-300 hover:bg-emerald-700/60'
                : 'rounded bg-amber-700/40 px-3 py-1 text-xs text-amber-300 hover:bg-amber-700/60'
            }
          >
            {row.captured ? 'Release' : 'Capture'}
          </button>
        </Field>
      </div>

      <div className="px-3 pt-3 pb-1 text-[10px] uppercase tracking-widest text-zinc-500">Fire on this guest</div>
      <div className="flex items-center gap-2 px-3 pb-2">
        <select className={inputCls} value={signal} onChange={(e) => setSignal(e.target.value)}>
          <option value="">— named event —</option>
          {events.map((ev) => (
            <option key={ev} value={ev}>
              {ev}
            </option>
          ))}
        </select>
        <button
          onClick={() => signal && void fireSignal(signal, id)}
          disabled={!signal}
          className="shrink-0 rounded bg-zinc-800 px-2 py-1 text-xs text-zinc-200 hover:bg-zinc-700 disabled:opacity-40"
        >
          Fire
        </button>
      </div>

      <div className="flex items-center gap-2 px-3 pb-2">
        <select className={inputCls} value={scanChar} onChange={(e) => setScanChar(e.target.value)}>
          <option value="">— scan as character —</option>
          {cast.map((c) => (
            <option key={c.id} value={c.id}>
              {c.id}
            </option>
          ))}
        </select>
        <button
          onClick={() => scanChar && void scanAs(scanChar, id)}
          disabled={!scanChar}
          className="shrink-0 rounded bg-zinc-800 px-2 py-1 text-xs text-zinc-200 hover:bg-zinc-700 disabled:opacity-40"
        >
          Scan
        </button>
      </div>

      <div className="flex flex-col gap-1.5 px-3 pb-4 pt-1">
        <button onClick={dm} className="w-full rounded border border-zinc-800 px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-800">
          ✉️ Direct message
        </button>
        <button
          onClick={() => {
            setPerspective(id)
            setTab(CockpitTab.Chat)
          }}
          className="w-full rounded border border-indigo-900 px-3 py-1.5 text-xs text-indigo-300 hover:bg-indigo-950"
          data-testid={`inspector-view-as-${id}`}
        >
          👁 View + act as {row.name}
        </button>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Character (cast) inspector
// ---------------------------------------------------------------------------

function CharacterInspector({ id }: { id: string }) {
  const member = useCockpit((s) => s.cast.find((c) => c.id === id))
  const beats = useCockpit((s) => s.beats)
  const fireBeat = useCockpit((s) => s.fireBeat)
  const setPerspective = useCockpit((s) => s.setPerspective)
  const setTab = useCockpit((s) => s.setTab)

  // Beats this character owns are keyed `Owner.beat`.
  const owned = beats.filter((b) => b.startsWith(`${id}.`))

  return (
    <div className="h-full overflow-auto">
      <div className="border-b border-zinc-800 px-3 py-2">
        <div className="text-[10px] uppercase tracking-widest text-zinc-500">Character</div>
        <div className="flex items-center gap-2">
          <span className="text-sm font-semibold text-zinc-100">{id}</span>
          <FactionPill faction={member?.faction ?? null} />
          <button
            onClick={() => {
              setPerspective(id)
              setTab(CockpitTab.Chat)
            }}
            className="ml-auto rounded border border-indigo-900 px-2 py-0.5 text-[10px] text-indigo-300 hover:bg-indigo-950"
            title={`Speak as ${id} in every room`}
          >
            👁 act as
          </button>
        </div>
      </div>

      <div className="px-3 pt-3 pb-1 text-[10px] uppercase tracking-widest text-zinc-500">Owned beats</div>
      {owned.length === 0 ? (
        <p className="px-3 py-2 text-xs text-zinc-600">
          No beats owned by this character. Use the Director tab to fire any global beat, or the Chat composer to speak as {id}.
        </p>
      ) : (
        <div className="flex flex-col gap-1 px-3 pb-4">
          {owned.map((b) => (
            <button
              key={b}
              onClick={() => void fireBeat(b)}
              className="flex items-center justify-between rounded-lg border border-zinc-800 px-3 py-1.5 text-left text-sm hover:bg-zinc-800"
            >
              <span className="truncate text-zinc-200">{b.slice(id.length + 1)}</span>
              <span className="shrink-0 text-xs text-indigo-400">fire →</span>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Faction inspector
// ---------------------------------------------------------------------------

function FactionInspector({ id }: { id: string }) {
  const faction = useCockpit((s) => s.factions.find((f) => f.id === id))
  const roster = useCockpit((s) => s.roster)
  const reveal = useCockpit((s) => s.reveal)
  const broadcast = useCockpit((s) => s.broadcast)
  const select = useCockpit((s) => s.select)
  const [cue, setCue] = useState('')

  if (!faction) return <Empty msg="This faction is no longer in the world." />
  const nameOf = (gid: string) => roster.find((r) => r.id === gid)?.name ?? gid

  return (
    <div className="h-full overflow-auto">
      <div className="border-b border-zinc-800 px-3 py-2">
        <div className="text-[10px] uppercase tracking-widest text-zinc-500">Faction</div>
        <div className="flex items-center gap-2">
          <FactionPill faction={faction.id} />
          {faction.hidden && (
            <span className={faction.revealed ? 'text-xs text-red-400' : 'text-xs text-zinc-600'}>
              {faction.revealed ? 'exposed' : 'hidden'}
            </span>
          )}
        </div>
      </div>

      <div className="py-1 text-xs">
        {faction.ethos && (
          <div className="px-3 py-1">
            <span className="text-zinc-500">Ethos </span>
            <span className="text-zinc-300">{faction.ethos}</span>
          </div>
        )}
        {faction.rival && (
          <div className="px-3 py-1">
            <span className="text-zinc-500">Rival </span>
            <span className="text-zinc-300">{faction.rival}</span>
          </div>
        )}
        <div className="px-3 py-1 text-zinc-500">{faction.members.length} members</div>
      </div>

      {faction.hidden && !faction.revealed && (
        <div className="px-3 pb-2">
          <button
            onClick={() => void reveal(faction.id)}
            className="w-full rounded bg-red-900/50 px-3 py-1.5 text-xs text-red-200 hover:bg-red-900/70"
          >
            ⚠️ Expose this faction
          </button>
        </div>
      )}

      <div className="px-3 pt-2 pb-1 text-[10px] uppercase tracking-widest text-zinc-500">Broadcast to faction</div>
      <div className="flex items-center gap-2 px-3 pb-2">
        <input
          className={inputCls}
          value={cue}
          placeholder="cue (e.g. rally)"
          onChange={(e) => setCue(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && cue.trim() && void broadcast(`faction(${faction.id})`, cue.trim())}
        />
        <button
          onClick={() => cue.trim() && void broadcast(`faction(${faction.id})`, cue.trim())}
          className="shrink-0 rounded bg-zinc-800 px-2 py-1 text-xs text-zinc-200 hover:bg-zinc-700"
        >
          Send
        </button>
      </div>

      <div className="px-3 pt-2 pb-1 text-[10px] uppercase tracking-widest text-zinc-500">Members</div>
      <div className="flex flex-col px-1 pb-4">
        {faction.members.length === 0 && <div className="px-2 py-1 text-xs text-zinc-600">No members.</div>}
        {faction.members.map((gid) => (
          <button
            key={gid}
            onClick={() => select({ kind: SelectionKind.Guest, id: gid })}
            className="rounded px-2 py-1 text-left text-sm text-zinc-300 hover:bg-zinc-800"
          >
            {nameOf(gid)}
          </button>
        ))}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Location inspector
// ---------------------------------------------------------------------------

function LocationInspector({ id }: { id: string }) {
  const location = useCockpit((s) => s.locations.find((l) => l.id === id))
  const roster = useCockpit((s) => s.roster)
  const broadcast = useCockpit((s) => s.broadcast)
  const select = useCockpit((s) => s.select)
  const [cue, setCue] = useState('')

  if (!location) return <Empty msg="This location is no longer in the world." />
  const nameOf = (gid: string) => roster.find((r) => r.id === gid)?.name ?? gid

  return (
    <div className="h-full overflow-auto">
      <div className="border-b border-zinc-800 px-3 py-2">
        <div className="text-[10px] uppercase tracking-widest text-zinc-500">Location</div>
        <div className="flex items-center gap-2">
          <span className="text-sm font-semibold text-zinc-100">{location.label || location.id}</span>
          {location.prison && <span className="text-xs text-red-400">prison</span>}
        </div>
        <div className="text-[10px] text-zinc-600">{location.id}</div>
      </div>

      <div className="px-3 py-1 text-xs text-zinc-500">{location.occupants.length} here</div>

      <div className="px-3 pt-2 pb-1 text-[10px] uppercase tracking-widest text-zinc-500">Broadcast to location</div>
      <div className="flex items-center gap-2 px-3 pb-2">
        <input
          className={inputCls}
          value={cue}
          placeholder="cue (e.g. lights_out)"
          onChange={(e) => setCue(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && cue.trim() && void broadcast(`location(${location.id})`, cue.trim())}
        />
        <button
          onClick={() => cue.trim() && void broadcast(`location(${location.id})`, cue.trim())}
          className="shrink-0 rounded bg-zinc-800 px-2 py-1 text-xs text-zinc-200 hover:bg-zinc-700"
        >
          Send
        </button>
      </div>

      <div className="px-3 pt-2 pb-1 text-[10px] uppercase tracking-widest text-zinc-500">Occupants</div>
      <div className="flex flex-col px-1 pb-4">
        {location.occupants.length === 0 && <div className="px-2 py-1 text-xs text-zinc-600">Empty.</div>}
        {location.occupants.map((gid) => (
          <button
            key={gid}
            onClick={() => select({ kind: SelectionKind.Guest, id: gid })}
            className="rounded px-2 py-1 text-left text-sm text-zinc-300 hover:bg-zinc-800"
          >
            {nameOf(gid)}
          </button>
        ))}
      </div>
    </div>
  )
}

function Empty({ msg }: { msg: string }) {
  return <div className="grid h-full place-items-center px-4 text-center text-xs text-zinc-600">{msg}</div>
}

export function CockpitInspector() {
  const selection = useCockpit((s) => s.selection)
  return (
    <div className="h-full w-full bg-zinc-950 text-zinc-100">
      {selection === null ? (
        <Empty msg="Select a guest, cast member, faction, or location to inspect and edit it." />
      ) : selection.kind === SelectionKind.Guest ? (
        <GuestInspector key={selection.id} id={selection.id} />
      ) : selection.kind === SelectionKind.Character ? (
        <CharacterInspector key={selection.id} id={selection.id} />
      ) : selection.kind === SelectionKind.Faction ? (
        <FactionInspector key={selection.id} id={selection.id} />
      ) : (
        <LocationInspector key={selection.id} id={selection.id} />
      )}
    </div>
  )
}
