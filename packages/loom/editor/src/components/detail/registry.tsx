// Phase 3 of the Loom IDE redesign §4.3: DetailRegistry.
// One renderer per FocusRef kind. The same component is mounted by
// every projection sink (panel / popover / modal / side), so a
// character detail looks identical whether you peek at it from the
// timeline (popover) or pin it into the dock (panel).

import type { FocusRef } from '@/store/focus'
import {
  eventTag,
  EVENT_COLORS,
  type LedgerEvent,
} from '@/components/runner/event-format'
import { useActiveHead } from '@/components/runner/use-head'
import type { PlayEnvelopeMeta, PlayTrackInfo } from '@/lib/sync'

// Property is named `for` (renamed from `ref`) so the React lint rule
// doesn't confuse it with `React.Ref` access.
type Props = { for: FocusRef }

export function DetailFor(props: Props) {
  const target = props.for
  switch (target.kind) {
    case 'envelope':
      return <EnvelopeDetail idx={target.idx} />
    case 'character':
      return <CharacterDetail name={target.name} />
    case 'track':
      return <TrackDetail track={target.track} />
    case 'world-key':
      return <WorldKeyDetail wkey={target.key} />
    case 'beat':
      return <BeatDetail name={target.name} />
  }
}

// ---------------------------------------------------------------------------
// Shared chrome
// ---------------------------------------------------------------------------

function Header({ kind, title, subtitle }: { kind: string; title: string; subtitle?: string }) {
  return (
    <div className="px-3 py-2 border-b border-white/10">
      <div className="text-[10px] uppercase tracking-widest text-zinc-500">{kind}</div>
      <div className="text-zinc-100 text-sm font-semibold truncate">{title}</div>
      {subtitle && <div className="text-zinc-500 text-xs truncate">{subtitle}</div>}
    </div>
  )
}

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex gap-3 px-3 py-1 border-b border-white/5 hover:bg-white/5 text-xs">
      <div className="text-zinc-500 w-28 shrink-0">{label}</div>
      <div className="text-zinc-200 min-w-0 break-words">{value}</div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

function EnvelopeDetail({ idx }: { idx: number }) {
  const play = useActiveHead()
  if (!play) return <Empty msg="No play session." />
  const event = (play.transcript as LedgerEvent[])[idx]
  const meta: PlayEnvelopeMeta | undefined = play.meta?.[idx]
  if (!event) return <Empty msg={`Envelope #${idx} not found.`} />
  const [tag, body] = eventTag(event)
  const colour = EVENT_COLORS[tag] ?? '#78909c'
  const track = play.tracks?.find((t) => t.id === meta?.track)
  return (
    <div className="h-full overflow-auto bg-zinc-950 font-mono">
      <Header
        kind={`Envelope #${idx}`}
        title={tag}
        subtitle={track ? `track ${track.label} (${track.kind})` : `track ${meta?.track ?? '?'}`}
      />
      <div className="px-3 py-2 flex items-center gap-2 border-b border-white/5">
        <span
          className="inline-block w-3 h-3 rounded-sm"
          style={{ backgroundColor: colour }}
        />
        <span style={{ color: colour }}>{tag}</span>
      </div>
      <Row
        label="cause"
        value={
          meta?.cause != null ? (
            <span className="text-zinc-300">#{meta.cause}</span>
          ) : (
            <span className="text-zinc-600">—</span>
          )
        }
      />
      {Object.entries(body).map(([k, v]) => (
        <Row key={k} label={k} value={typeof v === 'string' ? v : JSON.stringify(v)} />
      ))}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Character
// ---------------------------------------------------------------------------

function CharacterDetail({ name }: { name: string }) {
  const play = useActiveHead()
  if (!play) return <Empty msg="No play session." />
  const events = play.transcript as LedgerEvent[]
  const meta: PlayEnvelopeMeta[] = play.meta ?? []
  const tracks: PlayTrackInfo[] = play.tracks ?? []
  const track = tracks.find((t) => t.label === name)
  const appearances: { idx: number; tag: string; text: string }[] = []
  for (let i = 0; i < events.length; i++) {
    const [tag, body] = eventTag(events[i])
    const speakers = (body.speakers as string[]) ?? [body.speaker as string]
    if (tag === 'Dialogue' && speakers.includes(name)) {
      appearances.push({ idx: i, tag, text: String(body.text ?? '') })
    } else if (meta[i]?.track != null && track?.id === meta[i].track) {
      // Anything on the character's row counts too.
      appearances.push({ idx: i, tag, text: '' })
    }
  }
  const knowledge = (play.world ?? []).filter(([k]) => k.startsWith(`${name}.`))
  return (
    <div className="h-full overflow-auto bg-zinc-950 font-mono">
      <Header
        kind="Character"
        title={name}
        subtitle={
          track
            ? `${appearances.length} envelopes · ${knowledge.length} world keys`
            : `${knowledge.length} world keys`
        }
      />
      <Section title="World">
        {knowledge.length === 0 ? (
          <Empty msg="No world entries." />
        ) : (
          knowledge.map(([k, v]) => (
            <Row key={k} label={k.slice(name.length + 1)} value={v} />
          ))
        )}
      </Section>
      <Section title="Appearances">
        {appearances.length === 0 ? (
          <Empty msg="No appearances yet." />
        ) : (
          appearances.slice(-50).map((a) => (
            <Row
              key={a.idx}
              label={`#${a.idx}`}
              value={
                <span>
                  <span className="text-zinc-500 mr-1">{a.tag}</span>
                  {a.text}
                </span>
              }
            />
          ))
        )}
      </Section>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Track / World key / Beat
// ---------------------------------------------------------------------------

function TrackDetail({ track }: { track: number }) {
  const play = useActiveHead()
  if (!play) return <Empty msg="No play session." />
  const info = play.tracks?.find((t) => t.id === track)
  if (!info) return <Empty msg={`Track ${track} not found.`} />
  const meta: PlayEnvelopeMeta[] = play.meta ?? []
  const events = play.transcript as LedgerEvent[]
  const onRow: number[] = []
  for (let i = 0; i < meta.length; i++) if (meta[i]?.track === track) onRow.push(i)
  return (
    <div className="h-full overflow-auto bg-zinc-950 font-mono">
      <Header kind={`Track #${track}`} title={info.label} subtitle={info.kind} />
      <Row label="envelopes" value={String(onRow.length)} />
      <Section title="Recent">
        {onRow
          .slice(-30)
          .reverse()
          .map((i) => {
            const [tag] = eventTag(events[i])
            return <Row key={i} label={`#${i}`} value={tag} />
          })}
      </Section>
    </div>
  )
}

function WorldKeyDetail({ wkey }: { wkey: string }) {
  const play = useActiveHead()
  if (!play) return <Empty msg="No play session." />
  const current = (play.world ?? []).find(([k]) => k === wkey)?.[1] ?? '—'
  const events = play.transcript as LedgerEvent[]
  const writes: { idx: number; value: string }[] = []
  for (let i = 0; i < events.length; i++) {
    const [tag, body] = eventTag(events[i])
    if (tag === 'WorldSet' && body.key === wkey) {
      writes.push({ idx: i, value: String(body.value ?? '') })
    } else if (tag === 'KnowledgeChanged') {
      const k = `${body.character ?? ''}.knows.${body.field ?? ''}`
      if (k === wkey) writes.push({ idx: i, value: String(body.value ?? '') })
    }
  }
  return (
    <div className="h-full overflow-auto bg-zinc-950 font-mono">
      <Header kind="World key" title={wkey} subtitle={`${writes.length} writes`} />
      <Row label="current" value={String(current)} />
      <Section title="History">
        {writes.length === 0 ? (
          <Empty msg="No writes yet." />
        ) : (
          writes
            .slice(-50)
            .reverse()
            .map((w) => <Row key={w.idx} label={`#${w.idx}`} value={w.value} />)
        )}
      </Section>
    </div>
  )
}

function BeatDetail({ name }: { name: string }) {
  const play = useActiveHead()
  if (!play) return <Empty msg="No play session." />
  const events = play.transcript as LedgerEvent[]
  const visits: number[] = []
  for (let i = 0; i < events.length; i++) {
    const [tag, body] = eventTag(events[i])
    if (tag === 'BeatEntered' && body.beat === name) visits.push(i)
  }
  return (
    <div className="h-full overflow-auto bg-zinc-950 font-mono">
      <Header kind="Beat" title={name} subtitle={`${visits.length} visits`} />
      <Section title="Entries">
        {visits.length === 0 ? (
          <Empty msg="Not yet visited." />
        ) : (
          visits.map((i) => <Row key={i} label={`#${i}`} value="entered" />)
        )}
      </Section>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="px-3 pt-3 pb-1 text-[10px] uppercase tracking-widest text-zinc-500">
        {title}
      </div>
      <div>{children}</div>
    </div>
  )
}

function Empty({ msg }: { msg: string }) {
  return <div className="px-3 py-2 text-zinc-600 text-xs italic">{msg}</div>
}
