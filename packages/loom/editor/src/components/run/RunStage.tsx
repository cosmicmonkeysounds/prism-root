//! Run mode — center stage: one cockpit, two sources. A **Sim ⇄ Live**
//! switch in the header picks the backend: Sim drives the local
//! in-browser `@loom/core` simulator (rehearsal — personas, lifecycle,
//! the raw ledger), Live drives the launched event over the mod SSE
//! (moderation). Both render the same cockpit pages (Chat / Roster /
//! World / Story / Director from `components/cockpit/tabs.tsx`); Sim
//! adds its Sim (setup) + Log pages. Launching/ending the live event
//! itself lives in Deploy mode (⌘3) — this stage only rehearses and
//! moderates. Owns the mod-stream lifecycle while Run mode is open.

import { useEffect, useState } from 'react'
import clsx from 'clsx'
import { useWorkspace } from '@/store/workspace'
import { useOperate } from '@/store/operate'
import { useSim, SimStatus } from '@/store/sim'
import { RunSource, useLiveRun, useRun } from '@/store/run'
import { useMode } from '@/store/mode'
import { CockpitTab, useCockpit } from '@/store/cockpit'
import { StoryGraphPanel } from '@/components/graph/StoryGraphPanel'
import { ChatTab, DirectorTab, RosterTab, WorldTab } from '@/components/cockpit/tabs'
import { RunCockpit } from '@/components/cockpit/providers'
import { SimSetupTab } from '@/components/sim/SimSetupTab'
import { SimLogTab } from '@/components/sim/SimLogTab'

const SIM_TABS: { id: CockpitTab; label: string }[] = [
  { id: CockpitTab.Sim, label: 'Sim' },
  { id: CockpitTab.Chat, label: 'Chat' },
  { id: CockpitTab.Roster, label: 'Roster' },
  { id: CockpitTab.World, label: 'World' },
  { id: CockpitTab.Story, label: 'Story' },
  { id: CockpitTab.Director, label: 'Director' },
  { id: CockpitTab.Log, label: 'Log' },
]

const LIVE_TABS: { id: CockpitTab; label: string }[] = [
  { id: CockpitTab.Chat, label: 'Chat' },
  { id: CockpitTab.Roster, label: 'Roster' },
  { id: CockpitTab.World, label: 'World' },
  { id: CockpitTab.Story, label: 'Story' },
  { id: CockpitTab.Director, label: 'Director' },
]

/** The Sim ⇄ Live segmented switch. Live needs a server project; the
 *  green dot marks a launched event waiting to be moderated. */
function SourceSwitch() {
  const source = useRun((s) => s.source)
  const setSource = useRun((s) => s.setSource)
  const projectId = useWorkspace((s) => s.projectId)
  const hasEvent = useOperate((s) => s.event !== null)

  return (
    <div className="flex items-center rounded-md border border-zinc-800 p-0.5" role="group" aria-label="Run source">
      {(
        [
          [RunSource.Sim, 'Sim', true],
          [RunSource.Live, 'Live', projectId !== null],
        ] as Array<[RunSource, string, boolean]>
      ).map(([id, label, enabled]) => (
        <button
          key={id}
          type="button"
          disabled={!enabled}
          aria-pressed={source === id}
          data-testid={`run-source-${id}`}
          onClick={() => setSource(id)}
          title={
            id === RunSource.Live && !enabled
              ? 'Live events need a server project (local folders rehearse in Sim)'
              : id === RunSource.Live
                ? 'Moderate the live event'
                : 'Rehearse on the local in-browser simulator'
          }
          className={clsx(
            'flex items-center gap-1.5 rounded px-2.5 py-0.5 text-xs font-medium transition-colors',
            source === id ? 'bg-indigo-500/20 text-indigo-200' : 'text-zinc-500 hover:text-zinc-200',
            !enabled && 'cursor-not-allowed opacity-40',
          )}
        >
          {label}
          {id === RunSource.Live && hasEvent && <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" aria-label="event live" />}
        </button>
      ))}
    </div>
  )
}

/** Compact "fire a named event from anywhere" control (header-resident).
 *  Reads whichever cockpit hosts it — the local sim or the live event. */
function QuickFire() {
  const events = useCockpit((s) => s.events)
  const live = useCockpit((s) => s.live)
  const fireSignal = useCockpit((s) => s.fireSignal)
  const [name, setName] = useState('')
  if (!live || events.length === 0) return null
  return (
    <span className="flex items-center gap-1">
      <select
        value={name}
        onChange={(e) => setName(e.target.value)}
        className="rounded border border-zinc-800 bg-zinc-950 px-1.5 py-0.5 text-[11px] text-zinc-300 outline-none focus:border-indigo-500"
        title="Fire a named event"
        data-testid="sim-quickfire-select"
      >
        <option value="">— event —</option>
        {events.map((ev) => (
          <option key={ev} value={ev}>
            {ev}
          </option>
        ))}
      </select>
      <button
        onClick={() => name && void fireSignal(name)}
        disabled={!name}
        className="rounded bg-indigo-600/80 px-2 py-0.5 text-[11px] text-white hover:bg-indigo-500 disabled:opacity-40"
        data-testid="sim-quickfire-button"
      >
        ⚡ fire
      </button>
    </span>
  )
}

/** Header chrome shared by both sources: source switch · pages · status. */
function StageChrome({
  tabs,
  status,
  children,
}: {
  tabs: { id: CockpitTab; label: string }[]
  status: string
  children: React.ReactNode
}) {
  const tab = useCockpit((s) => s.activeTab)
  const setTab = useCockpit((s) => s.setTab)
  return (
    <div className="flex h-full w-full flex-col bg-zinc-950 text-zinc-100">
      <div className="flex items-center justify-between gap-2 border-b border-zinc-800 px-2">
        <div className="flex items-center gap-2">
          <SourceSwitch />
          <div role="tablist" className="flex items-stretch">
            {tabs.map((t) => (
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
        </div>
        <span className="flex items-center gap-3 pr-1 text-xs text-zinc-500">
          <QuickFire />
          {status}
        </span>
      </div>
      <div className="min-h-0 flex-1">{children}</div>
    </div>
  )
}

function SimPane() {
  const status = useSim((s) => s.status)
  const tab = useSim((s) => s.activeTab)
  const statusText =
    status === SimStatus.Running ? '▶ simulating'
    : status === SimStatus.Paused ? '⏸ paused'
    : 'no simulation'
  return (
    <StageChrome tabs={SIM_TABS} status={statusText}>
      {tab === CockpitTab.Sim && <SimSetupTab />}
      {tab === CockpitTab.Chat && <ChatTab />}
      {tab === CockpitTab.Roster && <RosterTab />}
      {tab === CockpitTab.World && <WorldTab />}
      {tab === CockpitTab.Story && <StoryGraphPanel variant="run" />}
      {tab === CockpitTab.Director && <DirectorTab />}
      {tab === CockpitTab.Log && <SimLogTab />}
    </StageChrome>
  )
}

function LivePane() {
  const event = useOperate((s) => s.event)
  const connected = useOperate((s) => s.connected)
  const tab = useOperate((s) => s.activeTab)
  const setMode = useMode((s) => s.setMode)
  const statusText = event ? (connected ? '● live' : '○ reconnecting…') : 'no active event'

  return (
    <StageChrome tabs={LIVE_TABS} status={statusText}>
      {event === null ? (
        <div className="grid h-full w-full place-items-center p-8 text-center text-sm text-zinc-500">
          <div>
            <p>No live event for this project.</p>
            <button
              onClick={() => setMode('deploy')}
              className="mt-3 rounded-lg bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-500"
              data-testid="run-goto-deploy"
            >
              Launch one in Deploy (⌘3) →
            </button>
          </div>
        </div>
      ) : (
        <>
          {tab === CockpitTab.Chat && <ChatTab />}
          {tab === CockpitTab.Roster && <RosterTab />}
          {tab === CockpitTab.World && <WorldTab />}
          {tab === CockpitTab.Story && <StoryGraphPanel variant="run" />}
          {tab === CockpitTab.Director && <DirectorTab />}
        </>
      )}
    </StageChrome>
  )
}

export function RunStage() {
  const live = useLiveRun()
  const projectId = useWorkspace((s) => s.projectId)
  const init = useOperate((s) => s.init)
  const teardown = useOperate((s) => s.teardown)

  // The mod-stream lifecycle rides the *stage*, not the Live pane, so
  // flipping Sim ⇄ Live never reconnects — and the Live badge on the
  // source switch knows about the event while rehearsing.
  useEffect(() => {
    if (!projectId) return
    void init(projectId)
    return () => teardown()
  }, [projectId, init, teardown])

  return <RunCockpit>{live ? <LivePane /> : <SimPane />}</RunCockpit>
}
