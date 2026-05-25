import { Sidebar } from '@/components/files/Sidebar'
import { Editor } from '@/components/editor/Editor'
import { Tabs } from '@/components/editor/Tabs'
import { Breadcrumbs } from '@/components/editor/Breadcrumbs'
import { Canvas } from '@/components/canvas/Canvas'
import { SearchPanel } from '@/components/search/SearchPanel'
import { CloudPanel } from '@/components/cloud/CloudPanel'
import { RemoteEditor } from '@/components/cloud/RemoteEditor'
import { PlayPanel } from '@/components/cloud/PlayPanel'

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
