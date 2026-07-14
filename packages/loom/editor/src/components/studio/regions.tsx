// Per-mode region contents (Loom IDE redesign v3 — three modes).
//
// Each region (left rail / center stage / timeline dock) is a thin
// switch over the active `Mode` that composes the EXISTING leaf panels.
// Writing's center is itself a resizable split — the text editor and
// the story-graph node editor side by side, 1:1 over the same source.
// The properties tray lives in its own `PropertiesTray.tsx`.

import type { ReactNode } from 'react'
import { Allotment, LayoutPriority } from 'allotment'
import type { Mode } from '@/store/mode'

import { Sidebar } from '@/components/files/Sidebar'
import { Editor } from '@/components/editor/Editor'
import { Tabs } from '@/components/editor/Tabs'
import { Breadcrumbs } from '@/components/editor/Breadcrumbs'
import { BeatStrip } from '@/components/graph/BeatStrip'
import { EditingRail } from '@/components/graph/EditingRail'
import { StoryGraphPanel } from '@/components/graph/StoryGraphPanel'
import { CockpitRail } from '@/components/cockpit/Rail'
import { RunCockpit } from '@/components/cockpit/providers'
import { RunStage } from '@/components/run/RunStage'
import { DeployStage } from '@/components/deploy/DeployStage'

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

/** Writing's center: text ⇄ story graph, both live over the same
 *  source. Either pane snaps closed for a full-width text or canvas. */
function WritingStage({
  split,
  onSplit,
}: {
  split: [number, number]
  onSplit: (split: [number, number]) => void
}) {
  return (
    <Allotment
      defaultSizes={split}
      proportionalLayout={false}
      // Persist only clean, fully-visible samples (a snapped pane
      // reports 0 — keep the last open width so it restores).
      onChange={(s) => {
        if (s.length === 2 && s.every((n) => n > 0)) onSplit([s[0], s[1]])
      }}
    >
      <Allotment.Pane minSize={300} preferredSize={split[0]} snap>
        <Region>
          <EditorStack />
        </Region>
      </Allotment.Pane>
      <Allotment.Pane minSize={320} priority={LayoutPriority.High} snap>
        <Region>
          <StoryGraphPanel />
        </Region>
      </Allotment.Pane>
    </Allotment>
  )
}

export function LeftRail({ mode }: { mode: Mode }) {
  switch (mode) {
    case 'writing':
      // Files + project-wide Story Bin, tabbed.
      return <EditingRail />
    case 'run':
      return (
        <RunCockpit>
          <CockpitRail />
        </RunCockpit>
      )
    case 'deploy':
      // Deploy has no working rail — file browsing for reference only.
      return <Sidebar />
  }
}

export function CenterStage({
  mode,
  split,
  onSplit,
}: {
  mode: Mode
  split: [number, number]
  onSplit: (split: [number, number]) => void
}) {
  switch (mode) {
    case 'writing':
      return <WritingStage split={split} onSplit={onSplit} />
    case 'run':
      // One cockpit — the local simulator or the live event, per the
      // stage's Sim ⇄ Live source switch.
      return <RunStage />
    case 'deploy':
      return <DeployStage />
  }
}

export function TimelineDock({ mode }: { mode: Mode }) {
  // Writing facet — the selected beat's body as reorderable clips.
  if (mode === 'writing') return <BeatStrip />
  return null
}
