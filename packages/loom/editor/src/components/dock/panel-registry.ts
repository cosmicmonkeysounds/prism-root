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
  TranscriptPanelHost,
  ChoicesPanelHost,
  LedgerPanelHost,
  WorldPanelHost,
  TimelinePanelHost,
  InspectorPanelHost,
  GraphPanelHost,
  DetailPanelHost,
  CastPanelHost,
  BoothPanelHost,
  OutlinePanelHost,
  ReferencesPanelHost,
} from './panels'

export type PanelId =
  | 'files'
  | 'search'
  | 'editor'
  | 'canvas'
  | 'cloud'
  | 'remote'
  | 'play'
  | 'transcript'
  | 'choices'
  | 'ledger'
  | 'world'
  | 'timeline'
  | 'inspector'
  | 'graph'
  | 'detail'
  | 'cast'
  | 'booth'
  | 'outline'
  | 'references'

export type PanelKind = 'project' | 'author' | 'structure' | 'runner'

export const PANEL_TITLES: Record<PanelId, string> = {
  files: 'Files',
  search: 'Search',
  editor: 'Editor',
  canvas: 'Canvas',
  cloud: 'Cloud',
  remote: 'Remote',
  play: 'Play',
  transcript: 'Transcript',
  choices: 'Choices',
  ledger: 'Ledger',
  world: 'World',
  timeline: 'Timeline',
  inspector: 'Inspector',
  graph: 'Graph',
  detail: 'Detail',
  cast: 'Cast',
  booth: 'Booth',
  outline: 'Outline',
  references: 'References',
}

export const PANEL_KINDS: Record<PanelId, PanelKind> = {
  files: 'project',
  search: 'project',
  cloud: 'project',
  editor: 'author',
  canvas: 'structure',
  graph: 'structure',
  remote: 'author',
  play: 'runner',
  transcript: 'runner',
  choices: 'runner',
  ledger: 'runner',
  world: 'runner',
  timeline: 'runner',
  inspector: 'runner',
  detail: 'runner',
  cast: 'runner',
  booth: 'runner',
  outline: 'author',
  references: 'author',
}

export const PANEL_ORDER: PanelId[] = [
  'files',
  'search',
  'cloud',
  'editor',
  'remote',
  'canvas',
  'timeline',
  'transcript',
  'choices',
  'ledger',
  'world',
  'inspector',
  'detail',
  'graph',
  'cast',
  'booth',
  'outline',
  'references',
  'play',
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
  transcript: TranscriptPanelHost,
  choices: ChoicesPanelHost,
  ledger: LedgerPanelHost,
  world: WorldPanelHost,
  timeline: TimelinePanelHost,
  inspector: InspectorPanelHost,
  graph: GraphPanelHost,
  detail: DetailPanelHost,
  cast: CastPanelHost,
  booth: BoothPanelHost,
  outline: OutlinePanelHost,
  references: ReferencesPanelHost,
}
