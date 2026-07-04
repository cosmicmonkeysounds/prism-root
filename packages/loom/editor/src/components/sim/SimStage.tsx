//! Sim mode — center stage: the local rehearsal cockpit. The same pages
//! as Run (Chat / Roster / World / Story / Director, from
//! `components/cockpit/tabs.tsx`) plus two of its own: **Sim** (lifecycle
//! + personas + pending choices) and **Log** (the raw sim ledger). A
//! quick-fire control lives in the header so named events can be fired
//! from anywhere, whatever page is active.

import { useState } from 'react'
import clsx from 'clsx'
import { CockpitTab, useCockpit } from '@/store/cockpit'
import { SimStatus, useSim } from '@/store/sim'
import { StoryGraphPanel } from '@/components/graph/StoryGraphPanel'
import { ChatTab, DirectorTab, RosterTab, WorldTab } from '@/components/cockpit/tabs'
import { SimCockpit } from '@/components/cockpit/providers'
import { SimSetupTab } from './SimSetupTab'
import { SimLogTab } from './SimLogTab'

const TABS: { id: CockpitTab; label: string }[] = [
  { id: CockpitTab.Sim, label: 'Sim' },
  { id: CockpitTab.Chat, label: 'Chat' },
  { id: CockpitTab.Roster, label: 'Roster' },
  { id: CockpitTab.World, label: 'World' },
  { id: CockpitTab.Story, label: 'Story' },
  { id: CockpitTab.Director, label: 'Director' },
  { id: CockpitTab.Log, label: 'Log' },
]

/** Compact "fire a named event from anywhere" control (header-resident). */
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

function Stage() {
  const status = useSim((s) => s.status)
  const tab = useSim((s) => s.activeTab)
  const setTab = useSim((s) => s.setTab)

  const statusText =
    status === SimStatus.Running ? '▶ simulating'
    : status === SimStatus.Paused ? '⏸ paused'
    : 'no simulation'

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
        <span className="flex items-center gap-3 pr-1 text-xs text-zinc-500">
          <QuickFire />
          {statusText}
        </span>
      </div>
      <div className="min-h-0 flex-1">
        {tab === CockpitTab.Sim && <SimSetupTab />}
        {tab === CockpitTab.Chat && <ChatTab />}
        {tab === CockpitTab.Roster && <RosterTab />}
        {tab === CockpitTab.World && <WorldTab />}
        {tab === CockpitTab.Story && <StoryGraphPanel variant="run" />}
        {tab === CockpitTab.Director && <DirectorTab />}
        {tab === CockpitTab.Log && <SimLogTab />}
      </div>
    </div>
  )
}

export function SimStage() {
  return (
    <SimCockpit>
      <Stage />
    </SimCockpit>
  )
}
