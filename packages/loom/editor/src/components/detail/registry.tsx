// DetailRegistry (Loom IDE redesign §4.3): one renderer per FocusRef
// kind, mounted by every projection sink (panel / popover / modal /
// side). The play-state details (envelope / track / live world history)
// were removed with the runtime — authoring focus refs resolve to a
// lightweight identity card; the Properties tray + References panel
// carry the rich author-time detail.

import type { FocusRef } from '@/store/focus'

// Property is named `for` (not `ref`) so the React lint rule doesn't
// confuse it with `React.Ref` access.
type Props = { for: FocusRef }

export function DetailFor({ for: target }: Props) {
  switch (target.kind) {
    case 'character':
      return <Card kind="Character" title={target.name} hint="Pin drives the References panel." />
    case 'beat':
      return <Card kind="Beat" title={target.name} hint="Open the beat in the editor to edit it." />
    case 'world-key':
      return <Card kind="World key" title={target.key} hint="Referenced across this workspace." />
  }
}

function Card({ kind, title, hint }: { kind: string; title: string; hint: string }) {
  return (
    <div className="h-full overflow-auto bg-zinc-950 font-mono">
      <div className="px-3 py-2 border-b border-white/10">
        <div className="text-[10px] uppercase tracking-widest text-zinc-500">{kind}</div>
        <div className="text-zinc-100 text-sm font-semibold truncate">{title}</div>
      </div>
      <p className="px-3 py-2 text-zinc-600 text-xs italic">{hint}</p>
    </div>
  )
}
