// Phase 1 of the Loom IDE redesign v2: per-mode region contents.
//
// Each region (left rail / center stage / timeline dock) is a thin
// switch over the active `Mode` that composes the EXISTING leaf panels
// — no panel was rewritten for v2. The properties tray lives in its
// own `PropertiesTray.tsx` (Phase 2).

import type { ReactNode } from 'react'
import { Allotment } from 'allotment'
import type { Mode } from '@/store/mode'

import { Sidebar } from '@/components/files/Sidebar'
import { Editor } from '@/components/editor/Editor'
import { Tabs } from '@/components/editor/Tabs'
import { Breadcrumbs } from '@/components/editor/Breadcrumbs'
import { OutlinePanel } from '@/components/runner/Outline'
import { GraphPanel } from '@/components/runner/Graph'
import { TranscriptPanel } from '@/components/runner/Transcript'
import { ChoicesPanel } from '@/components/runner/Choices'
import { WorldPanel } from '@/components/runner/World'
import { CastPanel } from '@/components/runner/Cast'
import { TimelinePanel } from '@/components/runner/Timeline'
import { CloudPanel } from '@/components/cloud/CloudPanel'
import { BeatTimeline } from './BeatTimeline'
import { useSession } from '@/store/session'

/** Fills an allotment pane and clips overflow so leaf panels scroll. */
export function Region({ children }: { children: ReactNode }) {
  return (
    <div className="h-full w-full min-h-0 min-w-0 overflow-hidden bg-zinc-950">
      {children}
    </div>
  )
}

function EditorStack() {
  return (
    <div className="h-full w-full flex flex-col bg-zinc-950">
      <Tabs />
      <Breadcrumbs />
      <div className="flex-1 min-h-0">
        <Editor />
      </div>
    </div>
  )
}

/** A fixed two-row vertical split used inside a few center stages. */
function VSplit({ top, bottom }: { top: ReactNode; bottom: ReactNode }) {
  return (
    <Allotment vertical>
      <Allotment.Pane minSize={120}>
        <Region>{top}</Region>
      </Allotment.Pane>
      <Allotment.Pane minSize={120}>
        <Region>{bottom}</Region>
      </Allotment.Pane>
    </Allotment>
  )
}

function DeliverStage() {
  const status = useSession((s) => s.status)
  const wsName = useSession((s) => s.active?.meta.name ?? null)
  const relay = useSession((s) => s.relayUrl)
  const peers = useSession((s) => s.active?.peers.length ?? 0)
  return (
    <div className="h-full w-full overflow-auto bg-zinc-950 text-zinc-300 p-6 text-sm">
      <div className="text-[10px] uppercase tracking-widest text-zinc-500">Production</div>
      <h2 className="text-zinc-100 text-lg font-semibold mt-1">Deliver</h2>
      <p className="text-zinc-400 mt-2 max-w-prose">
        Export, share, and deploy this workspace. Manage the relay and
        workspaces from the left rail.
      </p>
      <dl className="mt-4 grid grid-cols-[8rem_1fr] gap-x-3 gap-y-1 text-xs">
        <dt className="text-zinc-500">relay</dt>
        <dd className="text-zinc-200 break-all">{relay}</dd>
        <dt className="text-zinc-500">status</dt>
        <dd className="text-zinc-200">{status}</dd>
        <dt className="text-zinc-500">workspace</dt>
        <dd className="text-zinc-200">{wsName ?? '—'}</dd>
        <dt className="text-zinc-500">collaborators</dt>
        <dd className="text-zinc-200">{peers}</dd>
      </dl>
      <ul className="mt-5 space-y-2 text-zinc-400">
        <li><code className="text-zinc-200">prism loom build</code> — vite build + relay binary.</li>
        <li><code className="text-zinc-200">prism loom serve</code> — single-binary deployment (editor + API + WS).</li>
      </ul>
      <p className="text-zinc-600 text-xs mt-6">
        Export / one-click deploy controls land in a later phase.
      </p>
    </div>
  )
}

export function LeftRail({ mode }: { mode: Mode }) {
  switch (mode) {
    case 'writing':
      return <Sidebar />
    case 'editing':
      // Story Bin stand-in until the dedicated beat/scene browser lands.
      return <OutlinePanel />
    case 'simulating':
      return <ChoicesPanel />
    case 'performing':
      return <CastPanel />
    case 'production':
      return <CloudPanel />
  }
  return null
}

export function CenterStage({ mode }: { mode: Mode }) {
  switch (mode) {
    case 'writing':
      return <EditorStack />
    case 'editing':
      return <GraphPanel />
    case 'simulating':
      return <VSplit top={<TranscriptPanel />} bottom={<WorldPanel />} />
    case 'performing':
      return <VSplit top={<TranscriptPanel />} bottom={<ChoicesPanel />} />
    case 'production':
      return <DeliverStage />
  }
  return null
}

export function TimelineDock({ mode }: { mode: Mode }) {
  // Editing facet — author beats, draggable to reorder (writes source).
  // Run facet — the live ledger (Simulating / Performing).
  if (mode === 'editing') return <BeatTimeline />
  return <TimelinePanel />
}
