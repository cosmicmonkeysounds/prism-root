// Custom React Flow node types for the story-graph canvas.
//
// Articy-style cards: a beat is a readable card (name · owner · preview
// lines · content counts), a file is a translucent container, entities
// are compact pills, unresolved targets are red ghost nodes. Sizing
// helpers live here too so the ELK pass and the renderer can never
// disagree about a node's box.

import { memo } from 'react'
import { Handle, Position, type Node, type NodeProps } from '@xyflow/react'
import clsx from 'clsx'
import type { GraphBeat, GraphEntity } from '@loom/core/lsp'
import { InlineEdit } from './body-nodes'
import {
  BEAT_EXPANDED_W,
  BEAT_W,
  END_H,
  END_W,
  ENTITY_H,
  ENTITY_W,
  GHOST_H,
  GHOST_W,
  entityGlyph,
} from './metrics'
import { WordBlockKind, type WordBlock } from './word-blocks'

// ---------------------------------------------------------------------------
// Node data payloads
// ---------------------------------------------------------------------------

export type BeatNodeData = {
  beat: GraphBeat
  /** Full word-block body when the card is expanded (null → compact). */
  blocks?: WordBlock[] | null
  /** Expand / collapse this card's word blocks (wired by the canvas). */
  onToggleExpand?: () => void
  /** Runtime overlay (Run mode / simulator). */
  visits?: number
  isCurrent?: boolean
  /** Word-block authoring (project view, `file`-structural beats only). */
  blocksEditable?: boolean
  /** Index of the block currently inline-editing, if any. */
  editingBlock?: number | null
  /** Raw source slice seeding the block editor. */
  blockEditSlice?: string | null
  onEditBlock?: (index: number) => void
  onCommitBlock?: (index: number, next: string) => void
  onCancelBlock?: () => void
  onBlockContextMenu?: (index: number, event: React.MouseEvent) => void
}

export type EntityNodeData = { entity: GraphEntity }
export type FileGroupData = {
  path: string
  beatCount: number
  /** Header-only compact node — children hidden, edges re-routed here. */
  collapsed?: boolean
  /** Collapse / expand this container (wired by the canvas). */
  onToggleCollapse?: () => void
  /** Runtime overlay: the current beat lives inside this collapsed file. */
  isCurrent?: boolean
}
export type GhostNodeData = { target: string }

export type BeatFlowNode = Node<BeatNodeData, 'beat'>

// ---------------------------------------------------------------------------
// Beat card
// ---------------------------------------------------------------------------

const STRUCTURAL_ACCENT: Record<GraphBeat['structural'], string> = {
  file: 'border-l-indigo-400',
  owned: 'border-l-teal-400',
  derived: 'border-l-fuchsia-400',
}

export const BeatNode = memo(function BeatNode({ data, selected }: NodeProps<BeatFlowNode>) {
  const { beat, blocks, onToggleExpand, visits, isCurrent } = data
  const expanded = blocks != null
  return (
    <div
      className={clsx(
        'rounded-md border border-white/10 bg-zinc-900/95 text-left shadow-md',
        'border-l-[3px]',
        STRUCTURAL_ACCENT[beat.structural],
        selected && 'ring-2 ring-sky-400/70',
        isCurrent && 'ring-2 ring-amber-300 animate-pulse',
        beat.shadowed && 'opacity-60',
      )}
      style={{ width: expanded ? BEAT_EXPANDED_W : BEAT_W }}
      data-testid={`graph-beat-${beat.key}`}
    >
      <Handle type="target" position={Position.Left} className="!bg-zinc-500 !border-zinc-300/40" />
      <div className="px-2.5 pt-2">
        <div className="flex items-center gap-1.5">
          {onToggleExpand !== undefined && (
            <button
              type="button"
              title={expanded ? 'Collapse word blocks' : 'Expand word blocks'}
              className="nodrag -ml-1 w-4 shrink-0 text-[10px] text-zinc-500 hover:text-zinc-200"
              onClick={(e) => {
                e.stopPropagation()
                onToggleExpand()
              }}
              onDoubleClick={(e) => e.stopPropagation()}
              data-testid={`graph-beat-expand-${beat.key}`}
            >
              {expanded ? '▾' : '▸'}
            </button>
          )}
          {beat.entry && <span title="entry beat" className="text-amber-300 text-[11px]">▶</span>}
          <span className="truncate text-[12px] font-semibold text-zinc-100">
            {beat.owner !== null && <span className="text-teal-300/90">{beat.owner}.</span>}
            {beat.name}
          </span>
          {beat.params.length > 0 && (
            <span className="text-[10px] text-zinc-500 shrink-0">({beat.params.join(', ')})</span>
          )}
          {beat.shadowed && (
            <span title="shadowed by a later beat with the same name" className="text-rose-400 text-[10px]">⚠</span>
          )}
          {beat.structural === 'derived' && (
            <span title="trait-shipped template instance" className="text-fuchsia-300 text-[10px]">◈</span>
          )}
          {typeof visits === 'number' && visits > 0 && (
            <span className="ml-auto shrink-0 rounded-full bg-amber-400/20 px-1.5 text-[10px] text-amber-200">
              {visits}
            </span>
          )}
        </div>
        {(beat.owner !== null || beat.cast.length > 0 || beat.setting !== null) && (
          <div className="mt-0.5 truncate text-[10px] text-zinc-500">
            {beat.cast.length > 0 && <span>◉ {beat.cast.join(', ')}</span>}
            {beat.setting !== null && <span>{beat.cast.length > 0 ? ' · ' : ''}▦ {beat.setting}</span>}
            {beat.owner !== null && beat.cast.length === 0 && beat.setting === null && (
              <span>owned by {beat.owner}</span>
            )}
          </div>
        )}
      </div>
      {expanded ? (
        <WordBlockList blocks={blocks} beatKey={beat.key} data={data} />
      ) : (
        beat.preview.length > 0 && (
          <div className="mt-1 px-2.5">
            {beat.preview.slice(0, 3).map((line, i) => (
              <div key={i} className="truncate text-[10px] leading-[15px] text-zinc-400/90">
                {line}
              </div>
            ))}
          </div>
        )
      )}
      <div className="mt-1 flex items-center gap-2 border-t border-white/5 px-2.5 py-1 text-[9px] text-zinc-500">
        {beat.counts.dialogues > 0 && <span>💬 {beat.counts.dialogues}</span>}
        {beat.counts.choices > 0 && <span className="text-emerald-400/80">◇ {beat.counts.choices}</span>}
        {beat.counts.diverts > 0 && <span>→ {beat.counts.diverts}</span>}
        {beat.tunnelReturn && <span title="tunnel — returns to caller" className="text-violet-300">↩</span>}
        <span className="ml-auto uppercase tracking-wide opacity-60">{beat.structural}</span>
      </div>
      <Handle type="source" position={Position.Right} className="!bg-indigo-400 !border-indigo-200/50" />
    </div>
  )
})

// ---------------------------------------------------------------------------
// Word blocks (expanded beat card body)
// ---------------------------------------------------------------------------

const BLOCK_TONE: Record<WordBlockKind, string> = {
  [WordBlockKind.Prose]: 'text-zinc-300',
  [WordBlockKind.Dialogue]: 'border-l-2 border-cyan-400/50 pl-1.5 text-zinc-300',
  [WordBlockKind.Directive]: 'font-mono text-orange-200/90',
  [WordBlockKind.Choice]: 'border-l-2 border-emerald-400/60 pl-1.5 text-zinc-200',
  [WordBlockKind.Branch]: 'font-mono text-amber-200/90',
  [WordBlockKind.Divert]: 'font-mono text-indigo-300',
  [WordBlockKind.Slot]: 'font-mono text-fuchsia-300',
}

function WordBlockList({
  blocks,
  beatKey,
  data,
}: {
  blocks: WordBlock[]
  beatKey: string
  data: BeatNodeData
}) {
  if (blocks.length === 0) {
    return <div className="mt-1 px-2.5 text-[10px] italic text-zinc-600">Empty beat.</div>
  }
  const editable = data.blocksEditable === true
  return (
    <div className="mt-1 flex flex-col gap-1 px-2.5">
      {blocks.map((b, i) => {
        if (editable && data.editingBlock === i && data.blockEditSlice != null) {
          return (
            <div key={i} style={b.depth > 0 ? { marginLeft: Math.min(b.depth, 4) * 10 } : undefined}>
              <InlineEdit
                initial={data.blockEditSlice}
                onCommit={(next) => data.onCommitBlock?.(i, next)}
                onCancel={() => data.onCancelBlock?.()}
              />
            </div>
          )
        }
        const blockEditable = editable && b.spanStart !== null
        return (
          <div
            key={i}
            className={clsx(
              'text-[10px] leading-[14px]',
              BLOCK_TONE[b.kind],
              blockEditable && 'cursor-text hover:bg-white/5',
            )}
            style={b.depth > 0 ? { marginLeft: Math.min(b.depth, 4) * 10 } : undefined}
            onDoubleClick={
              blockEditable
                ? (e) => {
                    // The block owns its double-click — don't drill in.
                    e.stopPropagation()
                    data.onEditBlock?.(i)
                  }
                : undefined
            }
            onContextMenu={
              editable && data.onBlockContextMenu !== undefined
                ? (e) => {
                    e.preventDefault()
                    e.stopPropagation()
                    data.onBlockContextMenu?.(i, e)
                  }
                : undefined
            }
            data-testid={`graph-block-${beatKey}-${i}`}
          >
            {b.label !== null && (
              <span
                className={clsx(
                  'mr-1',
                  b.kind === WordBlockKind.Dialogue && 'font-semibold tracking-wide text-cyan-200',
                  b.kind === WordBlockKind.Choice && 'text-emerald-300',
                  b.kind === WordBlockKind.Slot && 'text-fuchsia-400',
                )}
              >
                {b.label}
              </span>
            )}
            <span className="whitespace-pre-wrap break-words">{b.text}</span>
          </div>
        )
      })}
    </div>
  )
}

// ---------------------------------------------------------------------------
// File container
// ---------------------------------------------------------------------------

export const FileGroupNode = memo(function FileGroupNode({
  data,
  selected,
}: NodeProps<Node<FileGroupData, 'fileGroup'>>) {
  const collapsed = data.collapsed === true
  const header = (
    <div
      className={clsx(
        'flex items-center gap-2 bg-zinc-900/70 px-2.5 py-1.5',
        collapsed ? 'h-full rounded-lg' : 'rounded-t-lg border-b border-white/10',
      )}
    >
      {data.onToggleCollapse !== undefined && (
        <button
          type="button"
          title={collapsed ? 'Expand file' : 'Collapse file'}
          className="nodrag -ml-1 w-4 shrink-0 text-[10px] text-zinc-500 hover:text-zinc-200"
          onClick={(e) => {
            e.stopPropagation()
            data.onToggleCollapse!()
          }}
          onDoubleClick={(e) => e.stopPropagation()}
          data-testid={`graph-file-collapse-${data.path}`}
        >
          {collapsed ? '▸' : '▾'}
        </button>
      )}
      <span className="text-[10px] text-zinc-500">▤</span>
      <span className="truncate text-[11px] font-medium text-zinc-300">{data.path}</span>
      <span className="ml-auto text-[9px] text-zinc-600">
        {collapsed ? `${data.beatCount} beat${data.beatCount === 1 ? '' : 's'}` : data.beatCount}
      </span>
    </div>
  )
  if (collapsed) {
    // Compact leaf: handles let the re-routed edges anchor to this node.
    return (
      <div
        className={clsx(
          'h-full w-full rounded-lg border bg-zinc-900/90 shadow-md',
          selected ? 'border-sky-400/50' : 'border-white/10',
          data.isCurrent === true && 'ring-2 ring-amber-300 animate-pulse',
        )}
        data-testid={`graph-file-${data.path}`}
      >
        <Handle type="target" position={Position.Left} className="!bg-zinc-500 !border-zinc-300/40" />
        {header}
        <Handle type="source" position={Position.Right} className="!bg-indigo-400 !border-indigo-200/50" />
      </div>
    )
  }
  return (
    <div
      className={clsx(
        'h-full w-full rounded-lg border bg-zinc-800/20',
        selected ? 'border-sky-400/50' : 'border-white/10',
      )}
      data-testid={`graph-file-${data.path}`}
    >
      {header}
    </div>
  )
})

// ---------------------------------------------------------------------------
// Entity pill
// ---------------------------------------------------------------------------

export const EntityNode = memo(function EntityNode({
  data,
  selected,
}: NodeProps<Node<EntityNodeData, 'entity'>>) {
  const { entity } = data
  const { glyph, cls } = entityGlyph(entity.kind)
  return (
    <div
      className={clsx(
        'flex items-center gap-1.5 rounded-full border bg-zinc-900/95 px-2.5 shadow-sm',
        cls,
        selected && 'ring-2 ring-sky-400/70',
      )}
      style={{ width: ENTITY_W, height: ENTITY_H }}
      title={`${entity.kind} ${entity.name}`}
    >
      <Handle type="target" position={Position.Left} className="!bg-zinc-500 !border-zinc-300/40" />
      <span className="text-[11px]">{glyph}</span>
      <span className="truncate text-[11px] text-zinc-200">{entity.name}</span>
      {entity.hookCount > 0 && (
        <span className="ml-auto text-[9px] text-zinc-500" title={`${entity.hookCount} hooks`}>
          ⚡{entity.hookCount}
        </span>
      )}
      <Handle type="source" position={Position.Right} className="!bg-cyan-400 !border-cyan-200/50" />
    </div>
  )
})

// ---------------------------------------------------------------------------
// Ghost (unresolved target) + END terminal
// ---------------------------------------------------------------------------

export const GhostNode = memo(function GhostNode({
  data,
  selected,
}: NodeProps<Node<GhostNodeData, 'ghost'>>) {
  return (
    <div
      className={clsx(
        'rounded-md border border-dashed border-rose-400/60 bg-rose-950/40 px-2.5 py-1.5 text-left',
        selected && 'ring-2 ring-sky-400/70',
      )}
      style={{ width: GHOST_W, minHeight: GHOST_H }}
      title="divert resolves to no beat"
    >
      <Handle type="target" position={Position.Left} className="!bg-rose-400 !border-rose-200/50" />
      <div className="truncate text-[11px] font-medium text-rose-200">-&gt; {data.target}</div>
      <div className="text-[9px] text-rose-300/70">unresolved</div>
    </div>
  )
})

export const EndNode = memo(function EndNode({ selected }: NodeProps<Node<Record<string, never>, 'end'>>) {
  return (
    <div
      className={clsx(
        'grid place-items-center rounded-full border border-zinc-500/60 bg-zinc-950 text-[11px] font-semibold tracking-widest text-zinc-300',
        selected && 'ring-2 ring-sky-400/70',
      )}
      style={{ width: END_W, height: END_H }}
    >
      <Handle type="target" position={Position.Left} className="!bg-zinc-500 !border-zinc-300/40" />
      END
    </div>
  )
})
