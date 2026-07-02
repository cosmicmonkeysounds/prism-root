// Custom node types for the beat DRILL-IN view — the level where the
// canvas is 1:1 with the language. Each body item renders as its own
// card: prose, dialogue blocks, directives, choice options, branch
// heads (`<if:>` / `<match:>` / `<each visit>` / `<after:>`), divert
// exits, and slot holes. Pixel-Crushers-style: options and branches
// fan out, everything else chains downward.

import { memo, useEffect, useRef, useState } from 'react'
import { Handle, Position, type Node, type NodeProps } from '@xyflow/react'
import clsx from 'clsx'
import type { LoomSpan } from '@/lib/loom-ast'
import { BODY_W, BRANCH_W, EXIT_W, START_W } from './metrics'

export type SourceAnchor = { uri: string; span: LoomSpan } | null

/** Shared payload: every body node can point back at authored source. */
type Anchored = {
  anchor: SourceAnchor
  /** In-place editing (wired by the canvas for `file`-structural beats). */
  editing?: boolean
  editSlice?: string | null
  onCommitEdit?: (next: string) => void
  onCancelEdit?: () => void
}

/**
 * The in-node source editor: a textarea over the item's raw `.loom`
 * slice (indentation included for multi-line blocks — what you edit IS
 * the text). Enter commits (Shift+Enter for a newline), Esc cancels,
 * blur commits.
 */
function InlineEdit({
  initial,
  onCommit,
  onCancel,
}: {
  initial: string
  onCommit: (next: string) => void
  onCancel: () => void
}) {
  const [value, setValue] = useState(initial)
  const ref = useRef<HTMLTextAreaElement>(null)
  useEffect(() => {
    const el = ref.current
    if (el === null) return
    el.focus()
    el.select()
    el.style.height = 'auto'
    el.style.height = `${el.scrollHeight}px`
  }, [])
  const commit = () => {
    if (value !== initial) onCommit(value)
    else onCancel()
  }
  return (
    <textarea
      ref={ref}
      value={value}
      onChange={(e) => {
        setValue(e.target.value)
        e.target.style.height = 'auto'
        e.target.style.height = `${e.target.scrollHeight}px`
      }}
      onBlur={commit}
      onKeyDown={(e) => {
        e.stopPropagation()
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault()
          commit()
        } else if (e.key === 'Escape') {
          e.preventDefault()
          onCancel()
        }
      }}
      onMouseDown={(e) => e.stopPropagation()}
      className="nodrag nopan w-full resize-none rounded border border-sky-400/50 bg-zinc-950 px-1.5 py-1 font-mono text-[11px] leading-[15px] text-zinc-100 outline-none"
      spellCheck={false}
    />
  )
}

/** Render the inline editor when a node is in editing state. */
function maybeEdit(data: Anchored): React.ReactNode | null {
  if (data.editing !== true || data.editSlice == null) return null
  return (
    <InlineEdit
      initial={data.editSlice}
      onCommit={data.onCommitEdit ?? (() => undefined)}
      onCancel={data.onCancelEdit ?? (() => undefined)}
    />
  )
}

export type BodyStartData = Anchored & {
  title: string
  params: string[]
  cast: string[]
  setting: string | null
}
export type BodyTextData = Anchored & { text: string; scene?: boolean }
export type BodyDialogueData = Anchored & { speaker: string; lines: string[]; parenthetical: string | null }
export type BodyDirectiveData = Anchored & { raw: string }
export type BodyChoiceData = Anchored & { text: string; sticky: boolean; suppressed: string | null }
export type BodyBranchData = Anchored & { label: string }
export type BodyExitData = Anchored & {
  form: 'divert' | 'tunnel' | 'return' | 'end'
  target: string | null
  /** Resolved graph key when the target names a real beat. */
  resolved: string | null
}
export type BodySlotData = Anchored & { name: string }

const cardBase =
  'rounded-md border border-white/10 bg-zinc-900/95 text-left shadow-sm px-2.5 py-1.5'

function Ports() {
  return (
    <>
      <Handle type="target" position={Position.Top} className="!bg-zinc-500 !border-zinc-300/40" />
      <Handle type="source" position={Position.Bottom} className="!bg-zinc-400 !border-zinc-200/40" />
    </>
  )
}

export const BodyStartNode = memo(function BodyStartNode({
  data,
}: NodeProps<Node<BodyStartData, 'bodyStart'>>) {
  return (
    <div className={clsx(cardBase, 'border-l-[3px] border-l-indigo-400')} style={{ width: START_W }}>
      <div className="text-[12px] font-semibold text-zinc-100">
        == {data.title}
        {data.params.length > 0 && <span className="text-zinc-400">({data.params.join(', ')})</span>}
      </div>
      {(data.cast.length > 0 || data.setting !== null) && (
        <div className="text-[10px] text-zinc-500">
          {data.cast.length > 0 && <>◉ {data.cast.join(', ')}</>}
          {data.setting !== null && <> · ▦ {data.setting}</>}
        </div>
      )}
      <Handle type="source" position={Position.Bottom} className="!bg-indigo-400 !border-indigo-200/50" />
    </div>
  )
})

export const BodyTextNode = memo(function BodyTextNode({
  data,
  selected,
}: NodeProps<Node<BodyTextData, 'bodyText'>>) {
  return (
    <div
      className={clsx(cardBase, selected && 'ring-2 ring-sky-400/70', data.scene === true && 'uppercase tracking-wide')}
      style={{ width: BODY_W }}
    >
      <Ports />
      {maybeEdit(data) ?? (
        <div className="text-[11px] leading-[14px] text-zinc-300 whitespace-pre-wrap">{data.text}</div>
      )}
    </div>
  )
})

export const BodyDialogueNode = memo(function BodyDialogueNode({
  data,
  selected,
}: NodeProps<Node<BodyDialogueData, 'bodyDialogue'>>) {
  return (
    <div
      className={clsx(cardBase, 'border-l-[3px] border-l-cyan-400/70', selected && 'ring-2 ring-sky-400/70')}
      style={{ width: BODY_W }}
    >
      <Ports />
      {maybeEdit(data) ?? (
        <>
          <div className="text-[10px] font-semibold tracking-wide text-cyan-200">
            {data.speaker}
            {data.parenthetical !== null && (
              <span className="ml-1 font-normal text-zinc-500">({data.parenthetical})</span>
            )}
          </div>
          {data.lines.map((l, i) => (
            <div key={i} className="mt-0.5 text-[11px] leading-[15px] text-zinc-300 whitespace-pre-wrap">
              {l}
            </div>
          ))}
        </>
      )}
    </div>
  )
})

export const BodyDirectiveNode = memo(function BodyDirectiveNode({
  data,
  selected,
}: NodeProps<Node<BodyDirectiveData, 'bodyDirective'>>) {
  return (
    <div
      className={clsx(
        'rounded-xl border border-orange-300/30 bg-orange-950/30 px-2.5 py-1 font-mono text-[10px] text-orange-200/90',
        selected && 'ring-2 ring-sky-400/70',
      )}
      style={{ maxWidth: BODY_W, minWidth: data.editing === true ? BODY_W : undefined }}
    >
      <Ports />
      {maybeEdit(data) ?? <span className="block break-words">{data.raw}</span>}
    </div>
  )
})

export const BodyChoiceNode = memo(function BodyChoiceNode({
  data,
  selected,
}: NodeProps<Node<BodyChoiceData, 'bodyChoice'>>) {
  return (
    <div
      className={clsx(cardBase, 'border-l-[3px] border-l-emerald-400/80', selected && 'ring-2 ring-sky-400/70')}
      style={{ width: BODY_W }}
    >
      <Ports />
      {maybeEdit(data) ?? (
        <>
          <div className="flex items-start gap-1.5">
            <span className="text-[11px] text-emerald-300" title={data.sticky ? 'sticky (+)' : 'once (*)'}>
              {data.sticky ? '+' : '*'}
            </span>
            <span className="text-[11px] leading-[15px] text-zinc-200">{data.text}</span>
          </div>
          {data.suppressed !== null && (
            <div className="pl-4 text-[9px] text-zinc-500">[{data.suppressed}]</div>
          )}
        </>
      )}
    </div>
  )
})

export const BodyBranchNode = memo(function BodyBranchNode({
  data,
  selected,
}: NodeProps<Node<BodyBranchData, 'bodyBranch'>>) {
  return (
    <div
      className={clsx(
        'rounded-md border border-amber-300/40 bg-amber-950/30 px-2.5 py-1.5 font-mono text-[10px] text-amber-200',
        selected && 'ring-2 ring-sky-400/70',
      )}
      style={{ minWidth: BRANCH_W }}
    >
      <Ports />
      <span className="block truncate">{data.label}</span>
    </div>
  )
})

export const BodyExitNode = memo(function BodyExitNode({
  data,
  selected,
}: NodeProps<Node<BodyExitData, 'bodyExit'>>) {
  const text =
    data.form === 'end' ? '-> END'
    : data.form === 'return' ? '<- return'
    : data.form === 'tunnel' ? `(${data.target ?? '?'}) ->`
    : `-> ${data.target ?? '?'}`
  const broken = data.form === 'divert' && data.resolved === null
  return (
    <div
      className={clsx(
        'rounded-full border px-3 py-1.5 text-[11px] font-medium',
        data.form === 'end'
          ? 'border-zinc-500/60 bg-zinc-950 text-zinc-300'
          : broken
            ? 'border-rose-400/60 border-dashed bg-rose-950/40 text-rose-200'
            : 'border-indigo-400/50 bg-indigo-950/40 text-indigo-200 hover:bg-indigo-900/50',
        selected && 'ring-2 ring-sky-400/70',
      )}
      style={{ minWidth: EXIT_W }}
      title={data.resolved !== null ? `double-click: open ${data.resolved}` : undefined}
    >
      <Handle type="target" position={Position.Top} className="!bg-zinc-500 !border-zinc-300/40" />
      <span className="block truncate text-center">{text}</span>
    </div>
  )
})

export const BodySlotNode = memo(function BodySlotNode({
  data,
  selected,
}: NodeProps<Node<BodySlotData, 'bodySlot'>>) {
  return (
    <div
      className={clsx(
        'rounded-md border border-dashed border-fuchsia-400/50 bg-fuchsia-950/30 px-2.5 py-1.5 text-[11px] text-fuchsia-200',
        selected && 'ring-2 ring-sky-400/70',
      )}
      style={{ minWidth: BRANCH_W }}
    >
      <Ports />
      slot: {data.name}
    </div>
  )
})
