//! Operate (Run) mode — center stage: the live admin cockpit. Six pages
//! over one mod stream: Event (launch / codes / controls), then the
//! shared cockpit pages — Chat / Roster / World / Story / Director
//! (`components/cockpit/tabs.tsx`) — which Sim mode reuses against its
//! local simulator. Owns the mod-stream lifecycle for the mode.

import { useEffect } from 'react'
import clsx from 'clsx'
import { useWorkspace } from '@/store/workspace'
import { useOperate } from '@/store/operate'
import { CockpitTab } from '@/store/cockpit'
import { StoryGraphPanel } from '@/components/graph/StoryGraphPanel'
import { ChatTab, DirectorTab, RosterTab, WorldTab } from '@/components/cockpit/tabs'
import { OperateCockpit } from '@/components/cockpit/providers'
import { EventPanel } from './EventPanel'

const TABS: { id: CockpitTab; label: string }[] = [
  { id: CockpitTab.Event, label: 'Event' },
  { id: CockpitTab.Chat, label: 'Chat' },
  { id: CockpitTab.Roster, label: 'Roster' },
  { id: CockpitTab.World, label: 'World' },
  { id: CockpitTab.Story, label: 'Story' },
  { id: CockpitTab.Director, label: 'Director' },
]

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
        (Local folders can be authored — rehearse them in Sim mode; live events run on server projects.)
      </div>
    )
  }

  return (
    <OperateCockpit>
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
          {tab === CockpitTab.Event && <EventPanel />}
          {tab === CockpitTab.Chat && <ChatTab />}
          {tab === CockpitTab.Roster && <RosterTab />}
          {tab === CockpitTab.World && <WorldTab />}
          {tab === CockpitTab.Story && <StoryGraphPanel variant="run" />}
          {tab === CockpitTab.Director && <DirectorTab />}
        </div>
      </div>
    </OperateCockpit>
  )
}
