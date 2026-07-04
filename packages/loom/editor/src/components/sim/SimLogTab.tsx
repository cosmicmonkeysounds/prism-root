//! Sim mode — the Log page: the raw sim ledger, one row per `SimEvent`,
//! newest at the bottom. This is the writer's x-ray: every action line,
//! dialogue, world write, hook firing, and diagnostic the engine
//! produced, exactly as the server would journal them. Formatting is
//! driven off the `SimEventType` enum so a new event kind fails the
//! exhaustiveness check here instead of rendering blank.

import { useEffect, useRef } from 'react'
import clsx from 'clsx'
import { SimEventType, type SimEvent } from '@loom/core/sim'
import { useSim, type SimLogEntry } from '@/store/sim'

/** Tone class per event type (grouped by family). */
function toneOf(type: SimEventType): string {
  switch (type) {
    case SimEventType.Dialogue:
    case SimEventType.Chat:
      return 'text-cyan-300'
    case SimEventType.Action:
    case SimEventType.Ambient:
      return 'text-zinc-300'
    case SimEventType.BeatEntered:
      return 'text-indigo-300'
    case SimEventType.ChoicePrompted:
    case SimEventType.Respond:
      return 'text-emerald-300'
    case SimEventType.Signal:
    case SimEventType.Broadcast:
    case SimEventType.Directive:
      return 'text-orange-300'
    case SimEventType.WorldSet:
    case SimEventType.RelationshipChanged:
      return 'text-amber-200/90'
    case SimEventType.Diagnostic:
      return 'text-rose-400'
    default:
      return 'text-zinc-400'
  }
}

/** One-line human rendering of a sim event. */
function describe(e: SimEvent): string {
  switch (e.type) {
    case SimEventType.AccountCreated:
      return `${e.name} joined as ${e.role} (${e.person})`
    case SimEventType.Joined:
      return `${e.person} joined ${e.faction}`
    case SimEventType.Defected:
      return `${e.person} defected ${e.from ?? 'unaligned'} → ${e.to}`
    case SimEventType.Betrayed:
      return `${e.person} secretly serves ${e.secret}`
    case SimEventType.FactionRevealed:
      return `${e.faction} exposed`
    case SimEventType.Scanned:
      return `${e.scanner} scanned ${e.person}`
    case SimEventType.Captured:
      return `${e.person} captured → ${e.location}${e.by ? ` by ${e.by}` : ''}`
    case SimEventType.Released:
      return `${e.person} released from ${e.location}`
    case SimEventType.Escaped:
      return `${e.person} escaped`
    case SimEventType.Arrived:
      return `${e.person} → ${e.location}${e.from ? ` (from ${e.from})` : ''}`
    case SimEventType.Cast:
      return `${e.person} cast as ${e.role}`
    case SimEventType.Promoted:
      return `${e.person} promoted to ${e.role}`
    case SimEventType.WorldSet:
      return `${e.path} = ${e.value}`
    case SimEventType.RelationshipChanged:
      return `${e.subject}.${e.relation}.${e.object} = ${e.value}`
    case SimEventType.Broadcast:
      return `broadcast "${e.cue}" → ${e.scope || 'all'} (${e.audience.length || 'all'})`
    case SimEventType.Dialogue:
      return `${e.speaker}: ${e.text}`
    case SimEventType.Chat:
      return `${e.from} #${e.channel}: ${e.text}`
    case SimEventType.ChannelInvited:
      return `${e.person} invited to ${e.channel} by ${e.by}`
    case SimEventType.ChannelLeft:
      return `${e.person} left ${e.channel}`
    case SimEventType.Action:
      return e.text
    case SimEventType.Directive:
      return `<${e.verb}: ${e.args}>`
    case SimEventType.BeatEntered:
      return `== ${e.beat}`
    case SimEventType.ChoicePrompted:
      return `choice for ${e.person ?? 'story'}: ${e.options.join(' | ')}`
    case SimEventType.Respond:
      return `${e.to} ⇐ ${e.text}`
    case SimEventType.Signal:
      return `signal "${e.name}"${e.subject ? ` on ${e.subject}` : ''}`
    case SimEventType.Ambient:
      return `${e.source}: ${e.text}`
    case SimEventType.Tick:
      return `tick +${e.elapsedMs}ms`
    case SimEventType.Diagnostic:
      return e.message
  }
}

function clock(ts: number): string {
  const s = Math.floor(ts / 1000)
  return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`
}

function Row({ entry }: { entry: SimLogEntry }) {
  return (
    <div className="flex gap-2 border-b border-white/5 px-3 py-1 font-mono text-[11px] leading-[16px]">
      <span className="w-10 shrink-0 text-right text-zinc-600">{entry.seq}</span>
      <span className="w-12 shrink-0 text-zinc-600">{clock(entry.ts)}</span>
      <span className="w-36 shrink-0 truncate text-zinc-500">{entry.event.type}</span>
      <span className={clsx('min-w-0 flex-1 break-words', toneOf(entry.event.type))}>
        {describe(entry.event)}
      </span>
    </div>
  )
}

export function SimLogTab() {
  const log = useSim((s) => s.log)
  const endRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    endRef.current?.scrollIntoView({ block: 'end' })
  }, [log.length])
  return (
    <div className="h-full overflow-auto" data-testid="sim-log">
      {log.length === 0 && (
        <div className="p-4 text-sm text-zinc-600">The ledger is empty — start a simulation.</div>
      )}
      {log.map((entry) => (
        <Row key={entry.seq} entry={entry} />
      ))}
      <div ref={endRef} />
    </div>
  )
}
