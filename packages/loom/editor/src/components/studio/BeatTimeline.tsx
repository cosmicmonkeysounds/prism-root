// Phase 4 of the Loom IDE redesign v2 (docs/dev/loom-ide-redesign.md
// §13 Editing facet): author beats as draggable clips. Reads the
// active `.loom` file's AST and lays its beats out in source order as
// a horizontal sortable list; dragging one to reorder rewrites the
// source via `loom-parser::edit::move_beat` (through the wasm
// `apply_move_beat`). The §10 "edits round-trip to source" invariant
// in action — a drag in the timeline is a source edit.

import { useMemo } from 'react'
import clsx from 'clsx'
import {
  DndContext,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from '@dnd-kit/core'
import {
  SortableContext,
  arrayMove,
  horizontalListSortingStrategy,
  useSortable,
} from '@dnd-kit/sortable'
import { useWorkspace } from '@/store/workspace'
import { useFocus } from '@/store/focus'
import {
  bodyBreakdown,
  field,
  itemKind,
  itemName,
  itemPayload,
  useLoomEdit,
  useLoomParser,
} from '@/lib/loom-ast'

type BeatInfo = { name: string; size: number; kinds: number }

export function BeatTimeline() {
  const activePath = useWorkspace((s) => s.activePath)
  const contents = useWorkspace((s) =>
    s.activePath ? s.openFiles[s.activePath]?.contents : undefined,
  )
  const updateContents = useWorkspace((s) => s.updateContents)
  const pin = useFocus((s) => s.pin)
  const parse = useLoomParser()
  const edit = useLoomEdit()

  const isLoom = !!activePath && activePath.endsWith('.loom')
  const beats = useMemo<BeatInfo[]>(() => {
    if (!parse || !isLoom || contents == null) return []
    try {
      const ast = parse(contents).ast
      const out: BeatInfo[] = []
      for (const it of ast.items) {
        if (itemKind(it) !== 'beat') continue
        const body = field(itemPayload(it), 'body')
        out.push({
          name: itemName(it),
          size: Array.isArray(body) ? body.length : 0,
          kinds: bodyBreakdown(body).length,
        })
      }
      return out
    } catch {
      return []
    }
  }, [parse, isLoom, contents])

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
  )

  if (!isLoom) return <Empty>Open a `.loom` file to arrange beats.</Empty>
  if (!parse) return <Empty>Loading parser…</Empty>
  if (beats.length === 0) return <Empty>No beats in this file yet.</Empty>

  const names = beats.map((b) => b.name)

  const onDragEnd = (e: DragEndEvent) => {
    const { active, over } = e
    if (!over || active.id === over.id) return
    if (!edit || contents == null || !activePath) return
    const from = names.indexOf(String(active.id))
    const to = names.indexOf(String(over.id))
    if (from < 0 || to < 0) return
    const order = arrayMove(names, from, to)
    const moved = String(active.id)
    const pos = order.indexOf(moved)
    const successor = order[pos + 1]
    try {
      const next = successor
        ? edit.moveBeat(contents, moved, 'before', successor)
        : edit.moveBeat(contents, moved, 'end', '')
      if (next !== contents) updateContents(activePath, next)
    } catch {
      /* invalid move — leave source untouched */
    }
  }

  return (
    <div className="h-full flex flex-col bg-[#15191e]">
      <div className="h-7 px-2 flex items-center text-[11px] text-zinc-400 border-b border-white/10 shrink-0">
        <span className="text-zinc-300 font-medium">Beats</span>
        <span className="ml-2 text-zinc-600">{beats.length}</span>
        <span className="ml-auto text-zinc-600">drag to reorder → rewrites source</span>
      </div>
      <div className="flex-1 min-h-0 overflow-auto p-3">
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
          <SortableContext items={names} strategy={horizontalListSortingStrategy}>
            <div className="flex items-stretch gap-2 min-h-[64px]">
              {beats.map((b) => (
                <BeatChip key={b.name} beat={b} onPin={() => pin({ kind: 'beat', name: b.name })} />
              ))}
            </div>
          </SortableContext>
        </DndContext>
      </div>
    </div>
  )
}

function BeatChip({ beat, onPin }: { beat: BeatInfo; onPin: () => void }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: beat.name,
  })
  const width = Math.max(96, Math.min(240, 80 + beat.size * 10))
  return (
    <div
      ref={setNodeRef}
      style={{
        transform: transform ? `translateX(${transform.x}px)` : undefined,
        transition,
        width,
      }}
      className={clsx(
        'rounded-md border bg-indigo-500/15 border-indigo-400/30 p-2 flex flex-col cursor-grab select-none shrink-0',
        isDragging && 'opacity-60 ring-1 ring-blue-400 z-10',
      )}
      {...attributes}
      {...listeners}
      onClick={onPin}
    >
      <span className="text-[12px] text-indigo-100 font-medium truncate">{beat.name}</span>
      <span className="text-[10px] text-zinc-500">
        {beat.size} item{beat.size === 1 ? '' : 's'}
      </span>
    </div>
  )
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="h-full grid place-items-center bg-[#15191e] text-zinc-500 text-xs px-4 text-center">
      {children}
    </div>
  )
}
