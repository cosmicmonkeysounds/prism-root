// Non-component metadata for the dock panels. Lives in a separate
// module so the matching `panels.tsx` file (which exports only React
// components) plays nicely with `react-refresh`'s fast-refresh rule —
// the rule fires if a single file exports both components and
// non-component values.

import type { FunctionComponent } from 'react'
import type { IDockviewPanelProps } from 'dockview-react'
import {
  FilesPanel,
  SearchPanelHost,
  EditorPanel,
  CanvasPanel,
  CloudPanelHost,
  RemotePanel,
  PlayPanelHost,
} from './panels'

export type PanelId =
  | 'files'
  | 'search'
  | 'editor'
  | 'canvas'
  | 'cloud'
  | 'remote'
  | 'play'

export const PANEL_TITLES: Record<PanelId, string> = {
  files: 'Files',
  search: 'Search',
  editor: 'Editor',
  canvas: 'Canvas',
  cloud: 'Cloud',
  remote: 'Remote',
  play: 'Play',
}

export const PANEL_ORDER: PanelId[] = [
  'files',
  'search',
  'cloud',
  'editor',
  'remote',
  'play',
  'canvas',
]

export const panelComponents: Record<
  PanelId,
  FunctionComponent<IDockviewPanelProps>
> = {
  files: FilesPanel,
  search: SearchPanelHost,
  editor: EditorPanel,
  canvas: CanvasPanel,
  cloud: CloudPanelHost,
  remote: RemotePanel,
  play: PlayPanelHost,
}
