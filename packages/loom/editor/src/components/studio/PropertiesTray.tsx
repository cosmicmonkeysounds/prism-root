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
import { useSession } from '@/store/session'
import { InspectorPanel } from '@/components/runner/Inspector'
import { BoothPanel } from '@/components/runner/Booth'
import { ReferencesPanel } from '@/components/runner/References'
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
  useLoomParser,
  type LoomFileAst,
} from '@/lib/loom-ast'

type Tab = { id: string; label: string; node: ReactNode }

function tabsFor(mode: Mode): Tab[] {
  switch (mode) {
    case 'writing':
    case 'editing':
      return [
        { id: 'props', label: 'Properties', node: <AuthorProperties /> },
        { id: 'refs', label: 'References', node: <ReferencesPanel /> },
      ]
    case 'simulating':
      return [
        { id: 'props', label: 'Inspector', node: <InspectorPanel /> },
        { id: 'refs', label: 'References', node: <ReferencesPanel /> },
      ]
    case 'performing':
      return [
        { id: 'booth', label: 'Booth', node: <BoothPanel /> },
        { id: 'props', label: 'Inspector', node: <InspectorPanel /> },
      ]
    case 'production':
      return [
        { id: 'deploy', label: 'Deploy', node: <DeployProps /> },
        { id: 'props', label: 'Inspector', node: <InspectorPanel /> },
      ]
  }
}

export function PropertiesTray({ mode }: { mode: Mode }) {
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
  const parse = useLoomParser()

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

  return (
    <div className="h-full overflow-auto bg-zinc-950 font-mono">
      <div className="px-3 py-1.5 border-b border-white/5 text-[10px] uppercase tracking-widest text-zinc-600 truncate">
        {base(activePath)}
      </div>
      {item ? <ItemDetail item={item} onPin={pin} /> : <FileSummary ast={ast} />}
    </div>
  )
}

function ItemDetail({ item, onPin }: { item: unknown; onPin: (ref: FocusRef | null) => void }) {
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
          {contract.length === 0 ? (
            <Empty msg="No contract." />
          ) : (
            contract.map(([k, pv]) => <Row key={k} label={k} value={propText(pv)} />)
          )}
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
// Production · Deploy
// ---------------------------------------------------------------------------

function DeployProps() {
  const relay = useSession((s) => s.relayUrl)
  return (
    <div className="h-full overflow-auto bg-zinc-950 font-mono">
      <Header kind="Production" title="Deploy" />
      <Row label="relay" value={relay} />
      <Section title="Commands">
        <Row label="build" value="prism loom build" />
        <Row label="serve" value="prism loom serve" />
      </Section>
      <p className="px-3 py-2 text-zinc-600 text-xs italic">
        Phase 1/2 stub — deploy controls land in a later phase.
      </p>
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

function base(path: string): string {
  return path.split('/').pop() ?? path
}
