import type { FunctionComponent } from 'react'
import type { IDockviewPanelProps } from 'dockview-react'
import { Sidebar } from '@/components/files/Sidebar'
import { Editor } from '@/components/editor/Editor'
import { Tabs } from '@/components/editor/Tabs'
import { Breadcrumbs } from '@/components/editor/Breadcrumbs'
import { Canvas } from '@/components/canvas/Canvas'
import { SearchPanel } from '@/components/search/SearchPanel'

const FilesPanel = () => <Sidebar />

const SearchPanelHost = () => <SearchPanel />

const EditorPanel = () => (
  <div className="h-full w-full flex flex-col bg-zinc-950">
    <Tabs />
    <Breadcrumbs />
    <div className="flex-1 min-h-0">
      <Editor />
    </div>
  </div>
)

const CanvasPanel = () => (
  <div className="h-full w-full bg-zinc-950">
    <Canvas />
  </div>
)

export type PanelId = 'files' | 'search' | 'editor' | 'canvas'

export const PANEL_TITLES: Record<PanelId, string> = {
  files: 'Files',
  search: 'Search',
  editor: 'Editor',
  canvas: 'Canvas',
}

export const PANEL_ORDER: PanelId[] = ['files', 'search', 'editor', 'canvas']

export const panelComponents: Record<PanelId, FunctionComponent<IDockviewPanelProps>> = {
  files: FilesPanel,
  search: SearchPanelHost,
  editor: EditorPanel,
  canvas: CanvasPanel,
}
