// The global top bar (Loom IDE redesign v2 §17.5). Authoring-only: the
// app label, the open-folder name, and a per-mode layout reset. The
// play transport, relay status, and presence strip were removed with
// the runtime (play lives in the `core` server + `play` app).

import { useWorkspace } from '@/store/workspace'
import { useMode } from '@/store/mode'

export function TopBar() {
  const folder = useWorkspace((s) => s.root?.name ?? null)
  return (
    <header className="h-9 px-3 flex items-center gap-3 border-b border-white/10 bg-zinc-950 text-sm shrink-0">
      <span className="font-semibold tracking-wide">Loom</span>
      <span className="text-zinc-500 text-xs truncate max-w-[280px]">
        {folder ?? 'no folder open'}
      </span>
      <div className="ml-auto flex items-center gap-3">
        <ResetLayout />
      </div>
    </header>
  )
}

function ResetLayout() {
  const mode = useMode((s) => s.mode)
  const reset = useMode((s) => s.reset)
  return (
    <button
      type="button"
      onClick={() => reset(mode)}
      title="Reset this mode's layout"
      className="text-zinc-600 hover:text-zinc-300 text-xs"
    >
      ⤢
    </button>
  )
}
