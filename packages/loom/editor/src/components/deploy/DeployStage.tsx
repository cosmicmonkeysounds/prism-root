//! Deploy mode — center stage: where the live event's unique admin
//! controls live. Launch preview/live, pause/resume/reset/end, the join
//! codes + QR, and guest lookup — everything about *hosting* the event.
//! Rehearsing and moderating live in Run mode (⌘2); this mode owns the
//! event's existence. Owns the mod-stream lifecycle while open, so the
//! status cards and the Inspector tray stay live.

import { useEffect } from 'react'
import { useWorkspace } from '@/store/workspace'
import { useOperate } from '@/store/operate'
import { OperateCockpit } from '@/components/cockpit/providers'
import { EventPanel } from './EventPanel'

export function DeployStage() {
  const projectId = useWorkspace((s) => s.projectId)
  const init = useOperate((s) => s.init)
  const teardown = useOperate((s) => s.teardown)
  const event = useOperate((s) => s.event)
  const connected = useOperate((s) => s.connected)

  useEffect(() => {
    if (!projectId) return
    void init(projectId)
    return () => teardown()
  }, [projectId, init, teardown])

  if (!projectId) {
    return (
      <div className="grid h-full w-full place-items-center bg-zinc-950 p-8 text-center text-sm text-zinc-500">
        Open a server project from the launchpad to deploy events.
        <br />
        (Local folders can be authored and rehearsed in Run mode's Sim source; live events run on server projects.)
      </div>
    )
  }

  return (
    <OperateCockpit>
      <div className="flex h-full w-full flex-col bg-zinc-950 text-zinc-100">
        <div className="flex items-center justify-between border-b border-zinc-800 px-3 py-2">
          <span className="text-sm font-medium text-zinc-200">Event</span>
          <span className="text-xs text-zinc-500" data-testid="deploy-status">
            {event ? (connected ? '● live' : '○ reconnecting…') : 'no active event'}
          </span>
        </div>
        <div className="min-h-0 flex-1">
          <EventPanel />
        </div>
      </div>
    </OperateCockpit>
  )
}
