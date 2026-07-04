//! Sim mode — the Sim page: simulator lifecycle (start / pause / reset /
//! stop, with a stale-sources hint), the writer's personas (local guests
//! they act as), and every pending choice — per persona and global — so
//! "play a character making choices" is one click.

import { useState } from 'react'
import clsx from 'clsx'
import { SelectionKind, useCockpit } from '@/store/cockpit'
import { GLOBAL_CHOICE_KEY, SimStatus, useSim } from '@/store/sim'
import { useLspIndexGen } from '@/lib/lsp-index'
import { lspWorkspaceSync } from '@/lib/lsp-client'
import { FactionPill } from '@/components/cockpit/ui'
import { PendingChoice } from '@/components/cockpit/Inspector'
import { useInspect } from '@/components/cockpit/inspect'

function Card({ title, children }: { title?: string; children: React.ReactNode }) {
  return (
    <section className="rounded-xl border border-zinc-800 bg-zinc-950 p-3">
      {title && <div className="mb-2 text-[10px] uppercase tracking-widest text-zinc-500">{title}</div>}
      {children}
    </section>
  )
}

const btn = 'rounded px-3 py-1.5 text-sm text-white disabled:opacity-40'

function Lifecycle() {
  const status = useSim((s) => s.status)
  const error = useSim((s) => s.error)
  const compiledAt = useSim((s) => s.compiledAt)
  const entryBeat = useSim((s) => s.entryBeat)
  const ledgerLen = useSim((s) => s.ledgerLen)
  const start = useSim((s) => s.start)
  const pause = useSim((s) => s.pause)
  const resume = useSim((s) => s.resume)
  const reset = useSim((s) => s.reset)
  const stop = useSim((s) => s.stop)
  const fireBeat = useSim((s) => s.fireBeat)

  // Source-staleness: the index generation moved past what the sim was
  // compiled from — offer Reset to pick the edits up.
  useLspIndexGen((s) => s.gen) // re-render when indexing lands
  const stale =
    status !== SimStatus.Idle && lspWorkspaceSync().generation !== compiledAt

  return (
    <Card title="Simulation">
      <div className="flex flex-wrap items-center gap-2">
        {status === SimStatus.Idle ? (
          <button onClick={start} className={clsx(btn, 'bg-emerald-600 hover:bg-emerald-500')} data-testid="sim-start">
            ▶ Start simulation
          </button>
        ) : (
          <>
            {status === SimStatus.Running ? (
              <button onClick={pause} className={clsx(btn, 'bg-amber-600 hover:bg-amber-500')}>
                ⏸ Pause
              </button>
            ) : (
              <button onClick={resume} className={clsx(btn, 'bg-emerald-600 hover:bg-emerald-500')}>
                ▶ Resume
              </button>
            )}
            <button onClick={reset} className={clsx(btn, 'bg-zinc-700 hover:bg-zinc-600')} title="Rebuild from current sources">
              ↺ Reset
            </button>
            <button onClick={stop} className={clsx(btn, 'bg-zinc-800 hover:bg-zinc-700')}>
              ■ Stop
            </button>
            {entryBeat !== null && (
              <button
                onClick={() => void fireBeat(entryBeat)}
                className={clsx(btn, 'bg-indigo-600 hover:bg-indigo-500')}
                title="Fire the entry beat again"
              >
                ▶ Replay entry · {entryBeat}
              </button>
            )}
          </>
        )}
        {status !== SimStatus.Idle && (
          <span className="text-xs text-zinc-500">{ledgerLen} ledger events</span>
        )}
      </div>
      {stale && (
        <p className="pt-2 text-xs text-amber-400">
          Sources changed since this simulation was compiled — <button className="underline" onClick={reset}>Reset</button> to
          pick up the edits.
        </p>
      )}
      {error !== null && <p className="pt-2 text-xs text-rose-400">{error}</p>}
      {status === SimStatus.Idle && (
        <p className="pt-2 text-xs text-zinc-600">
          Runs the story engine right here in the editor — no server, no event, no account. Watch the
          Story tab light up as beats fire, act as personas making choices, and fire named events to
          see the story react.
        </p>
      )}
    </Card>
  )
}

function PersonaCard({ id }: { id: string }) {
  const row = useCockpit((s) => s.roster.find((r) => r.id === id))
  const selection = useCockpit((s) => s.selection)
  const inspect = useInspect()
  if (row === undefined) return null
  const selected = selection?.kind === SelectionKind.Guest && selection.id === id
  return (
    <div
      className={clsx(
        'rounded-lg border',
        selected ? 'border-indigo-500/60' : 'border-zinc-800',
      )}
    >
      <button
        onClick={() => inspect({ kind: SelectionKind.Guest, id })}
        className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-zinc-900/60"
        data-testid={`sim-persona-${id}`}
      >
        <span className="text-sm font-medium text-zinc-100">{row.name}</span>
        <FactionPill faction={row.trueFaction ?? row.faction} />
        <span className="text-xs text-zinc-500">{row.location ?? '—'}</span>
        {row.captured && <span className="text-xs text-red-400">🔒</span>}
        <span className="ml-auto text-xs text-zinc-400">{row.score} pts</span>
      </button>
      <PendingChoice person={id} />
    </div>
  )
}

function Personas() {
  const status = useSim((s) => s.status)
  const personas = useSim((s) => s.personas)
  const addPersona = useSim((s) => s.addPersona)
  const [name, setName] = useState('')

  if (status === SimStatus.Idle) return null

  const add = () => {
    addPersona(name)
    setName('')
  }

  return (
    <Card title="Personas — act as a guest">
      <div className="flex flex-col gap-2">
        {personas.map((id) => (
          <PersonaCard key={id} id={id} />
        ))}
        <div className="flex items-center gap-2">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && add()}
            placeholder="persona name"
            className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-sm outline-none focus:border-indigo-500"
            data-testid="sim-persona-name"
          />
          <button
            onClick={add}
            className="rounded bg-zinc-800 px-3 py-1 text-sm text-zinc-200 hover:bg-zinc-700"
            data-testid="sim-persona-add"
          >
            + Add persona
          </button>
        </div>
        <p className="text-xs text-zinc-600">
          Select a persona to open its Inspector — set its faction/location, have characters scan it,
          or DM it. Choices a persona is offered appear on its card; to speak <em>as a character</em>,
          use the Chat composer's “post as” picker.
        </p>
      </div>
    </Card>
  )
}

function GlobalChoices() {
  const has = useCockpit((s) => (s.choices[GLOBAL_CHOICE_KEY]?.length ?? 0) > 0)
  if (!has) return null
  return (
    <Card title="Story choice (unbound)">
      <PendingChoice person={GLOBAL_CHOICE_KEY} />
      <p className="px-3 text-xs text-zinc-600">
        This menu suspended with no participant bound — answering it resumes the main storyline.
      </p>
    </Card>
  )
}

export function SimSetupTab() {
  return (
    <div className="h-full overflow-auto p-3">
      <div className="mx-auto flex max-w-lg flex-col gap-4">
        <Lifecycle />
        <GlobalChoices />
        <Personas />
      </div>
    </div>
  )
}
