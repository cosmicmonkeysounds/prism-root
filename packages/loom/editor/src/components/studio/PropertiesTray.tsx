// Phase 2 of the Loom IDE redesign v2 (docs/dev/loom-ide-redesign.md
// §14): the Properties tray. A tabbed, context-sensitive right rail
// present in every mode.
//
// - Per-mode tab sets + a per-mode default for the Properties tab.
// - Author modes (Writing / Editing) follow the editor cursor: the
//   tray shows the beat / declaration the cursor is inside, sourced
//   from the parsed AST (the runtime `DetailFor` needs a play head, so
//   it can't serve author time).
// - Runtime modes reuse the existing focus-driven `InspectorPanel`.
//
// Editable fields write back to `.loom` source via `loom-parser::edit`
// — that wiring is Phase 4 (needs a wasm binding); this phase is the
// read surface.

import { useMemo, useState, type ReactNode } from 'react'
import clsx from 'clsx'
import type { Mode } from '@/store/mode'
import { useWorkspace } from '@/store/workspace'
import { useFocus, type FocusRef } from '@/store/focus'
import { ReferencesPanel } from '@/components/runner/References'
import { OperateInspector } from '@/components/operate/OperateInspector'
import {
  bodyBreakdown,
  declKindLabel,
  entries,
  field,
  findBeat,
  findDeclaration,
  itemAtLine,
  itemKind,
  itemPayload,
  propText,
  summarize,
  useLoomEdit,
  useLoomParser,
  type LoomFileAst,
} from '@/lib/loom-ast'

type Tab = { id: string; label: string; node: ReactNode }

function tabsFor(_mode: Mode): Tab[] {
  // Both author modes share the same tray: cursor-following properties
  // + the workspace References panel.
  return [
    { id: 'props', label: 'Properties', node: <AuthorProperties /> },
    { id: 'refs', label: 'References', node: <ReferencesPanel /> },
  ]
}

export function PropertiesTray({ mode }: { mode: Mode }) {
  // Run mode's tray is the live participant Inspector, not the author props.
  if (mode === 'operate') return <OperateInspector />
  const tabs = tabsFor(mode)
  const [active, setActive] = useState(tabs[0].id)
  const current = tabs.find((t) => t.id === active) ?? tabs[0]
  return (
    <div className="h-full w-full flex flex-col bg-zinc-950">
      <div role="tablist" className="h-8 flex items-stretch border-b border-white/10 shrink-0 px-1">
        {tabs.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={t.id === current.id}
            onClick={() => setActive(t.id)}
            className={clsx(
              'px-3 text-xs border-b-2 -mb-px transition-colors',
              t.id === current.id
                ? 'border-blue-400 text-zinc-100'
                : 'border-transparent text-zinc-500 hover:text-zinc-200',
            )}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div className="flex-1 min-h-0 overflow-hidden">{current.node}</div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Author-time properties — follows the cursor / pinned author ref
// ---------------------------------------------------------------------------

function AuthorProperties() {
  const activePath = useWorkspace((s) => s.activePath)
  const contents = useWorkspace((s) =>
    s.activePath ? s.openFiles[s.activePath]?.contents : undefined,
  )
  const cursor = useWorkspace((s) => s.cursor)
  const pinned = useFocus((s) => s.pinned)
  const pin = useFocus((s) => s.pin)
  const updateContents = useWorkspace((s) => s.updateContents)
  const parse = useLoomParser()
  const edit = useLoomEdit()

  const isLoom = !!activePath && activePath.endsWith('.loom')
  const result = useMemo(() => {
    if (!parse || !isLoom || contents == null) return null
    try {
      return parse(contents)
    } catch {
      return null
    }
  }, [parse, isLoom, contents])

  if (!activePath) return <Empty msg="No file open." />
  if (!isLoom) return <FileInfo path={activePath} contents={contents ?? ''} />
  if (!result) return <Empty msg={parse ? 'Parsing…' : 'Loading parser…'} />

  const ast = result.ast
  let item: unknown | null = null
  if (pinned?.kind === 'beat') item = findBeat(ast, pinned.name)
  else if (pinned?.kind === 'character') item = findDeclaration(ast, pinned.name)
  if (!item && cursor) item = itemAtLine(ast, cursor.line - 1)

  const path = activePath
  const src = contents
  const onSet =
    edit && src != null
      ? (beat: string, key: string, value: string) => {
          try {
            const next = edit.setBeatProperty(src, beat, key, value)
            if (next !== src) updateContents(path, next)
          } catch {
            /* invalid edit — leave source untouched */
          }
        }
      : undefined

  return (
    <div className="h-full overflow-auto bg-zinc-950 font-mono">
      <div className="px-3 py-1.5 border-b border-white/5 text-[10px] uppercase tracking-widest text-zinc-600 truncate">
        {base(path)}
      </div>
      {item ? (
        <ItemDetail item={item} onPin={pin} onSet={onSet} />
      ) : (
        <FileSummary ast={ast} />
      )}
    </div>
  )
}

function ItemDetail({
  item,
  onPin,
  onSet,
}: {
  item: unknown
  onPin: (ref: FocusRef | null) => void
  onSet?: (beat: string, key: string, value: string) => void
}) {
  const kind = itemKind(item)
  const data = itemPayload(item)
  const name = String(field(data, 'name') ?? '')

  if (kind === 'beat') {
    const contract = entries(field(data, 'contract'))
    const params = (field(data, 'params') as string[] | undefined) ?? []
    const breakdown = bodyBreakdown(field(data, 'body'))
    return (
      <div>
        <Header
          kind="Beat"
          title={name}
          subtitle={params.length ? `(${params.join(', ')})` : undefined}
          onPin={() => onPin({ kind: 'beat', name })}
        />
        <Section title="Contract">
          {contract.length === 0 && !onSet && <Empty msg="No contract." />}
          {contract.map(([k, pv]) =>
            onSet ? (
              <EditableRow key={k} label={k} value={propText(pv)} onCommit={(v) => onSet(name, k, v)} />
            ) : (
              <Row key={k} label={k} value={propText(pv)} />
            ),
          )}
          {onSet && <AddField onAdd={(k, v) => onSet(name, k, v)} />}
        </Section>
        <Section title="Body">
          {breakdown.length === 0 ? (
            <Empty msg="Empty beat." />
          ) : (
            breakdown.map((b) => <Row key={b.tag} label={b.tag} value={String(b.count)} />)
          )}
        </Section>
      </div>
    )
  }

  if (kind === 'declaration') {
    const raw = (field(data, 'body') as { text?: string }[] | undefined) ?? []
    const mixin = (field(data, 'mixin') as string[] | undefined) ?? []
    return (
      <div>
        <Header
          kind={declKindLabel(field(data, 'kind'))}
          title={name}
          subtitle={mixin.length ? `is ${mixin.join(', ')}` : undefined}
          onPin={() => onPin({ kind: 'character', name })}
        />
        <Section title="Body">
          {raw.length === 0 ? (
            <Empty msg="No body." />
          ) : (
            raw.slice(0, 24).map((l, i) => <Row key={i} label={`${i + 1}`} value={l.text ?? ''} />)
          )}
        </Section>
      </div>
    )
  }

  return <Header kind="Let" title={name || 'binding'} />
}

function FileSummary({ ast }: { ast: LoomFileAst }) {
  const s = summarize(ast)
  const props = entries(field(ast.header, 'properties'))
  const title = String(field(ast.header, 'title') ?? '(untitled)')
  return (
    <div>
      <Header kind="File" title={title} />
      {props.length > 0 && (
        <Section title="Header">
          {props.map(([k, pv]) => (
            <Row key={k} label={k} value={propText(pv)} />
          ))}
        </Section>
      )}
      <Section title="Structure">
        <Row label="beats" value={String(s.beats)} />
        {s.declarations.map(([k, c]) => (
          <Row key={k} label={k.toLowerCase()} value={String(c)} />
        ))}
        {s.lets > 0 && <Row label="let bindings" value={String(s.lets)} />}
      </Section>
      <p className="px-3 py-2 text-zinc-600 text-xs italic">
        Move the cursor into a beat or declaration to inspect it.
      </p>
    </div>
  )
}

function FileInfo({ path, contents }: { path: string; contents: string }) {
  return (
    <div className="h-full overflow-auto bg-zinc-950 font-mono">
      <Header kind="File" title={base(path)} subtitle={path} />
      <Row label="lines" value={String(contents ? contents.split('\n').length : 0)} />
      <Row label="chars" value={String(contents.length)} />
    </div>
  )
}

// ---------------------------------------------------------------------------
// Shared chrome
// ---------------------------------------------------------------------------

function Header({
  kind,
  title,
  subtitle,
  onPin,
}: {
  kind: string
  title: string
  subtitle?: string
  onPin?: () => void
}) {
  return (
    <div className="px-3 py-2 border-b border-white/10">
      <div className="text-[10px] uppercase tracking-widest text-zinc-500">{kind}</div>
      <button
        type="button"
        disabled={!onPin}
        onClick={onPin}
        className={clsx(
          'text-sm font-semibold truncate text-left w-full',
          onPin ? 'text-zinc-100 hover:text-blue-300' : 'text-zinc-100 cursor-default',
        )}
        title={onPin ? 'Pin (drives References)' : undefined}
      >
        {title}
      </button>
      {subtitle && <div className="text-zinc-500 text-xs truncate">{subtitle}</div>}
    </div>
  )
}

function Row({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex gap-3 px-3 py-1 border-b border-white/5 hover:bg-white/5 text-xs">
      <div className="text-zinc-500 w-28 shrink-0 truncate">{label}</div>
      <div className="text-zinc-200 min-w-0 break-words">{value}</div>
    </div>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div>
      <div className="px-3 pt-3 pb-1 text-[10px] uppercase tracking-widest text-zinc-500">
        {title}
      </div>
      <div>{children}</div>
    </div>
  )
}

function Empty({ msg }: { msg: string }) {
  return <div className="px-3 py-2 text-zinc-600 text-xs italic">{msg}</div>
}

function EditableRow({
  label,
  value,
  onCommit,
}: {
  label: string
  value: string
  onCommit: (v: string) => void
}) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(value)
  const [seenValue, setSeenValue] = useState(value)
  // Resync the draft when the upstream value changes and we're not
  // mid-edit — the "adjust state during render" pattern (not an effect).
  if (!editing && value !== seenValue) {
    setSeenValue(value)
    setDraft(value)
  }
  return (
    <div className="flex gap-3 px-3 py-1 border-b border-white/5 text-xs items-center">
      <div className="text-zinc-500 w-28 shrink-0 truncate">{label}</div>
      {editing ? (
        <input
          autoFocus
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => {
            setEditing(false)
            if (draft !== value) onCommit(draft)
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') e.currentTarget.blur()
            else if (e.key === 'Escape') {
              setDraft(value)
              setEditing(false)
            }
          }}
          className="flex-1 min-w-0 bg-zinc-900 border border-blue-400/40 rounded px-1 text-zinc-100 outline-none"
        />
      ) : (
        <button
          type="button"
          onClick={() => setEditing(true)}
          className="flex-1 min-w-0 text-left text-zinc-200 break-words hover:bg-white/5 rounded px-1"
          title="Click to edit — writes back to .loom source"
        >
          {value || <span className="text-zinc-600 italic">empty</span>}
        </button>
      )}
    </div>
  )
}

function AddField({ onAdd }: { onAdd: (key: string, value: string) => void }) {
  return (
    <button
      type="button"
      onClick={() => {
        const key = window.prompt('New property name (e.g. setting)')?.trim()
        if (!key) return
        const value = window.prompt(`Value for ${key}`)?.trim() ?? ''
        onAdd(key, value)
      }}
      className="mx-3 my-1 px-2 py-0.5 text-[11px] text-zinc-400 border border-dashed border-white/15 rounded hover:text-zinc-200 hover:border-white/30"
    >
      + field
    </button>
  )
}

function base(path: string): string {
  return path.split('/').pop() ?? path
}
