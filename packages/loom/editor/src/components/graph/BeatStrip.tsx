// Writing mode's bottom dock: the selected beat's body as a linear
// strip of clips (Resolve-style). Drag to reorder — the reorder rewrites
// `.loom` source through `moveBodyItem`; right-click deletes; the
// composer appends raw body lines. Selection follows the canvas.

import { useMemo, useState } from 'react'
import {
  DndContext,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from '@dnd-kit/core'
import { SortableContext, horizontalListSortingStrategy, useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import clsx from 'clsx'
import type { BodyItem } from '@loom/core/parser'
import { appendBodyLines, moveBodyItem, parse, removeBodyItem, EditError } from '@loom/core/parser'
import { docText, lspWorkspaceSync } from '@/lib/lsp-client'
import { applyEditsToUri, useStoryGraph } from '@/lib/story-graph'
import { openContextMenu } from '@/store/context-menu'
import { useGraph } from '@/store/graph'

export function BeatStrip() {
  const graph = useStoryGraph()
  const view = useGraph((s) => s.view)
  const selected = useGraph((s) => s.selected)
  const [error, setError] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 4 } }))

  const beatKey =
    view.kind === 'beat' ? view.beatKey : selected !== null && graph.beats.has(selected) ? selected : null
  const beat = beatKey !== null ? graph.beats.get(beatKey) : undefined
  const body = useMemo(() => {
    void graph // identity tracks every document change, so the body stays fresh
    return beatKey !== null ? lspWorkspaceSync().beatBody(beatKey) : null
  }, [beatKey, graph])

  if (beat === undefined || body === null) {
    return (
      <Dock>
        <span className="text-zinc-600">Select a beat on the canvas to edit its sequence.</span>
      </Dock>
    )
  }

  const editable = beat.structural === 'file' && beat.uri !== null
  const uri = beat.uri

  const fail = (e: unknown, fallback: string): void => {
    setError(e instanceof EditError ? e.message : fallback)
    window.setTimeout(() => setError(null), 4000)
  }

  const onDragEnd = async (event: DragEndEvent): Promise<void> => {
    if (!editable || uri === null) return
    const { active, over } = event
    if (over === null || active.id === over.id) return
    const from = Number(String(active.id).slice(5))
    const to = Number(String(over.id).slice(5))
    if (Number.isNaN(from) || Number.isNaN(to)) return
    const text = docText(uri)
    if (text === null) return
    try {
      const [file] = parse(text)
      const edits = moveBodyItem(text, file, beat.name, from, to)
      if (edits.length > 0) await applyEditsToUri(uri, edits)
    } catch (e) {
      fail(e, 'Reorder failed.')
    }
  }

  const onDelete = async (index: number): Promise<void> => {
    if (!editable || uri === null) return
    const text = docText(uri)
    if (text === null) return
    try {
      const [file] = parse(text)
      await applyEditsToUri(uri, removeBodyItem(text, file, beat.name, index))
    } catch (e) {
      fail(e, 'Delete failed.')
    }
  }

  const onAppend = async (): Promise<void> => {
    if (!editable || uri === null || draft.trim().length === 0) return
    const text = docText(uri)
    if (text === null) return
    try {
      const [file] = parse(text)
      await applyEditsToUri(uri, appendBodyLines(text, file, beat.name, [draft.trim()]))
      setDraft('')
    } catch (e) {
      fail(e, 'Could not append the line.')
    }
  }

  const ids = body.map((_, i) => `item-${i}`)

  return (
    <Dock>
      <div className="flex items-center gap-2 shrink-0 pr-2">
        <span className="text-[11px] font-medium text-zinc-200">{beat.key}</span>
        <span className="text-[9px] uppercase tracking-wide text-zinc-600">{beat.structural}</span>
        {error !== null && <span className="text-[10px] text-amber-300">{error}</span>}
      </div>
      <div className="flex-1 min-w-0 overflow-x-auto">
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={(e) => void onDragEnd(e)}>
          <SortableContext items={ids} strategy={horizontalListSortingStrategy}>
            <div className="flex items-stretch gap-1.5 py-1">
              {body.map((item, i) => (
                <Clip
                  key={ids[i]}
                  id={ids[i]!}
                  item={item}
                  editable={editable}
                  onDelete={() => void onDelete(i)}
                />
              ))}
              {body.length === 0 && <span className="text-[11px] text-zinc-600 py-2">empty beat</span>}
            </div>
          </SortableContext>
        </DndContext>
      </div>
      {editable && (
        <div className="shrink-0 pl-2">
          <input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void onAppend()
            }}
            placeholder="add a line…  (-> beat · * choice · <set: …> · prose)"
            className="w-64 rounded border border-white/10 bg-zinc-900 px-2 py-1 text-[11px] text-zinc-200 placeholder:text-zinc-600 focus:outline-none focus:border-sky-400/50"
            data-testid="beat-strip-composer"
          />
        </div>
      )}
    </Dock>
  )
}

function Dock({ children }: { children: React.ReactNode }) {
  return (
    <div className="h-full flex items-center gap-2 border-t border-white/10 bg-[#0f1115] px-2 text-zinc-400">
      {children}
    </div>
  )
}

// ---------------------------------------------------------------------------
// One clip
// ---------------------------------------------------------------------------

const KIND_STYLE: Record<string, { label: string; cls: string }> = {
  action: { label: 'prose', cls: 'border-zinc-500/40 text-zinc-300' },
  sceneHeading: { label: 'scene', cls: 'border-zinc-400/40 text-zinc-200' },
  dialogue: { label: 'line', cls: 'border-cyan-400/50 text-cyan-200' },
  choice: { label: 'choice', cls: 'border-emerald-400/50 text-emerald-200' },
  divert: { label: 'divert', cls: 'border-indigo-400/50 text-indigo-200' },
  directive: { label: 'fx', cls: 'border-orange-300/40 text-orange-200' },
  directiveBlock: { label: 'fx', cls: 'border-orange-300/40 text-orange-200' },
  conditional: { label: 'if', cls: 'border-amber-300/50 text-amber-200' },
  match: { label: 'match', cls: 'border-amber-300/50 text-amber-200' },
  eachVisit: { label: 'visits', cls: 'border-amber-300/50 text-amber-200' },
  afterMorph: { label: 'after', cls: 'border-amber-300/50 text-amber-200' },
  inlineLet: { label: 'let', cls: 'border-violet-300/40 text-violet-200' },
  metadata: { label: 'fence', cls: 'border-zinc-500/40 text-zinc-400' },
  slotPlaceholder: { label: 'slot', cls: 'border-fuchsia-400/50 text-fuchsia-200' },
}

function clipText(item: BodyItem): string {
  switch (item.kind) {
    case 'action':
    case 'sceneHeading':
    case 'metadata':
      return item.value.value
    case 'dialogue': {
      // Show the actual spoken text, not just the speaker.
      const first = item.value.body.find((c) => c.kind === 'action')
      const text = first !== undefined && first.kind === 'action' ? first.value.value : ''
      return text.length > 0 ? text : item.value.speaker
    }
    case 'choice':
      return `${item.value.sticky ? '+' : '*'} ${item.value.text}`
    case 'divert': {
      const d = item.value
      if (d.kind === 'end') return '-> END'
      if (d.kind === 'return') return '<-'
      const t = d.target
      const base = t.qualifier === null ? t.name : `${t.qualifier}.${t.name}`
      return d.kind === 'tunnel' ? `(${base}) ->` : `-> ${base}`
    }
    case 'directive':
      return item.value.raw
    case 'directiveBlock':
      return item.value.directive.raw
    case 'conditional':
      return `<if:> ${item.value.arms.length} arm${item.value.arms.length === 1 ? '' : 's'}`
    case 'match':
      return `<match: ${item.value.scrutinee}>`
    case 'eachVisit':
      return '<each visit>'
    case 'afterMorph':
      return `<after: ${item.value.condition}>`
    case 'inlineLet':
      return `<let: ${item.value.name}>`
    case 'slotPlaceholder':
      return `slot: ${item.value.name}`
    default:
      return 'item'
  }
}

function Clip({
  id,
  item,
  editable,
  onDelete,
}: {
  id: string
  item: BodyItem
  editable: boolean
  onDelete: () => void
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id,
    disabled: !editable,
  })
  const style = KIND_STYLE[item.kind] ?? KIND_STYLE.action!
  const text = clipText(item)
  const speaker = item.kind === 'dialogue' ? item.value.speaker : null
  return (
    <div
      ref={setNodeRef}
      {...attributes}
      {...listeners}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={clsx(
        'flex min-w-28 max-w-72 cursor-grab select-none flex-col justify-center rounded border bg-zinc-900/90 px-2 py-1',
        style.cls,
        isDragging && 'opacity-60 cursor-grabbing',
      )}
      onContextMenu={(e) => {
        e.preventDefault()
        if (!editable) return
        openContextMenu(
          [{ label: 'Delete item', kind: 'danger', onSelect: onDelete }],
          { x: e.clientX, y: e.clientY },
        )
      }}
      title={speaker !== null ? `${speaker}: ${text}` : text}
    >
      <span className="text-[8px] uppercase tracking-wider opacity-60">
        {style.label}
        {speaker !== null && <span className="ml-1 normal-case tracking-normal">· {speaker}</span>}
      </span>
      <span className="line-clamp-2 text-[10px] leading-[13px]">{text}</span>
    </div>
  )
}
