// Phase 1 of the Loom IDE redesign v2: per-mode region contents.
//
// Each region (left rail / center stage / timeline dock) is a thin
// switch over the active `Mode` that composes the EXISTING leaf panels
// — no panel was rewritten for v2. The properties tray lives in its
// own `PropertiesTray.tsx` (Phase 2).

import type { ReactNode } from 'react'
import type { Mode } from '@/store/mode'

import { Sidebar } from '@/components/files/Sidebar'
import { Editor } from '@/components/editor/Editor'
import { Tabs } from '@/components/editor/Tabs'
import { Breadcrumbs } from '@/components/editor/Breadcrumbs'
import { BeatStrip } from '@/components/graph/BeatStrip'
import { EditingRail } from '@/components/graph/EditingRail'
import { StoryGraphPanel } from '@/components/graph/StoryGraphPanel'
import { RoomsRail } from '@/components/operate/RoomsRail'
import { OperateStage } from '@/components/operate/OperateStage'

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

export function LeftRail({ mode }: { mode: Mode }) {
  switch (mode) {
    case 'writing':
      return <Sidebar />
    case 'editing':
      // Files + project-wide Story Bin, tabbed.
      return <EditingRail />
    case 'operate':
      return <RoomsRail />
  }
}

export function CenterStage({ mode }: { mode: Mode }) {
  switch (mode) {
    case 'writing':
      return <EditorStack />
    case 'editing':
      // The global story-graph node editor (project ⇄ beat drill-in).
      return <StoryGraphPanel />
    case 'operate':
      return <OperateStage />
  }
}

export function TimelineDock({ mode }: { mode: Mode }) {
  // Editing facet — the selected beat's body as reorderable clips.
  if (mode === 'editing') return <BeatStrip />
  return null
}
