import { Sidebar } from '@/components/files/Sidebar'
import { Editor } from '@/components/editor/Editor'
import { Tabs } from '@/components/editor/Tabs'
import { Breadcrumbs } from '@/components/editor/Breadcrumbs'
import { Canvas } from '@/components/canvas/Canvas'
import { SearchPanel } from '@/components/search/SearchPanel'
import { CloudPanel } from '@/components/cloud/CloudPanel'
import { RemoteEditor } from '@/components/cloud/RemoteEditor'
import { PlayPanel } from '@/components/cloud/PlayPanel'
import { TranscriptPanel } from '@/components/runner/Transcript'
import { ChoicesPanel } from '@/components/runner/Choices'
import { LedgerPanel } from '@/components/runner/Ledger'
import { WorldPanel } from '@/components/runner/World'
import { TimelinePanel } from '@/components/runner/Timeline'
import { InspectorPanel } from '@/components/runner/Inspector'
import { GraphPanel } from '@/components/runner/Graph'
import { DetailPanelHost as DetailHost } from '@/components/detail/DetailPanelHost'
import { CastPanel } from '@/components/runner/Cast'
import { BoothPanel } from '@/components/runner/Booth'
import { OutlinePanel } from '@/components/runner/Outline'
import { ReferencesPanel } from '@/components/runner/References'

export const FilesPanel = () => <Sidebar />

export const SearchPanelHost = () => <SearchPanel />

export const EditorPanel = () => (
  <div className="h-full w-full flex flex-col bg-zinc-950">
    <Tabs />
    <Breadcrumbs />
    <div className="flex-1 min-h-0">
      <Editor />
    </div>
  </div>
)

export const CanvasPanel = () => (
  <div className="h-full w-full bg-zinc-950">
    <Canvas />
  </div>
)

export const CloudPanelHost = () => <CloudPanel />

export const RemotePanel = () => (
  <div className="h-full w-full bg-zinc-950">
    <RemoteEditor />
  </div>
)

export const PlayPanelHost = () => (
  <div className="h-full w-full bg-zinc-950">
    <PlayPanel />
  </div>
)

export const TranscriptPanelHost = () => <TranscriptPanel />
export const ChoicesPanelHost = () => <ChoicesPanel />
export const LedgerPanelHost = () => <LedgerPanel />
export const WorldPanelHost = () => <WorldPanel />
export const TimelinePanelHost = () => <TimelinePanel />
export const InspectorPanelHost = () => <InspectorPanel />
export const GraphPanelHost = () => <GraphPanel />
export const DetailPanelHost = () => <DetailHost />
export const CastPanelHost = () => <CastPanel />
export const BoothPanelHost = () => <BoothPanel />
export const OutlinePanelHost = () => <OutlinePanel />
export const ReferencesPanelHost = () => <ReferencesPanel />
