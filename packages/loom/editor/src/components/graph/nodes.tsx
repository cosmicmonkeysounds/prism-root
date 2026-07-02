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
import { BEAT_W, END_H, END_W, ENTITY_H, ENTITY_W, GHOST_H, GHOST_W, entityGlyph } from './metrics'

// ---------------------------------------------------------------------------
// Node data payloads
// ---------------------------------------------------------------------------

export type BeatNodeData = {
  beat: GraphBeat
  /** Runtime overlay (Run mode / simulator). */
  visits?: number
  isCurrent?: boolean
}

export type EntityNodeData = { entity: GraphEntity }
export type FileGroupData = { path: string; beatCount: number }
export type GhostNodeData = { target: string; from: string }

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
  const { beat, visits, isCurrent } = data
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
      style={{ width: BEAT_W }}
      data-testid={`graph-beat-${beat.key}`}
    >
      <Handle type="target" position={Position.Left} className="!bg-zinc-500 !border-zinc-300/40" />
      <div className="px-2.5 pt-2">
        <div className="flex items-center gap-1.5">
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
      {beat.preview.length > 0 && (
        <div className="mt-1 px-2.5">
          {beat.preview.slice(0, 3).map((line, i) => (
            <div key={i} className="truncate text-[10px] leading-[15px] text-zinc-400/90">
              {line}
            </div>
          ))}
        </div>
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
// File container
// ---------------------------------------------------------------------------

export const FileGroupNode = memo(function FileGroupNode({
  data,
  selected,
}: NodeProps<Node<FileGroupData, 'fileGroup'>>) {
  return (
    <div
      className={clsx(
        'h-full w-full rounded-lg border bg-zinc-800/20',
        selected ? 'border-sky-400/50' : 'border-white/10',
      )}
    >
      <div className="flex items-center gap-2 rounded-t-lg border-b border-white/10 bg-zinc-900/70 px-2.5 py-1.5">
        <span className="text-[10px] text-zinc-500">▤</span>
        <span className="truncate text-[11px] font-medium text-zinc-300">{data.path}</span>
        <span className="ml-auto text-[9px] text-zinc-600">{data.beatCount}</span>
      </div>
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
