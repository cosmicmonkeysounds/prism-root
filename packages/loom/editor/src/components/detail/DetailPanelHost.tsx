// Phase 3 of the Loom IDE redesign §4.3: the `panel` projection sink.
// Renders whatever is currently pinned in the focus store; clears
// itself when focus is released.

import { useFocus } from '@/store/focus'
import { DetailFor } from './registry'

export function DetailPanelHost() {
  const pinned = useFocus((s) => s.pinned)
  const clear = useFocus((s) => s.pin)
  if (!pinned) {
    return (
      <div className="h-full grid place-items-center bg-zinc-950 text-zinc-500 text-xs px-4 text-center">
        <div>
          <div>Detail</div>
          <div className="text-zinc-600 mt-1 max-w-xs">
            Click any envelope, character, beat, world key, or track
            row to pin it here.
          </div>
        </div>
      </div>
    )
  }
  return (
    <div className="h-full w-full flex flex-col bg-zinc-950">
      <div className="flex justify-end px-2 py-1 border-b border-white/10">
        <button
          type="button"
          onClick={() => clear(null)}
          className="text-zinc-500 hover:text-zinc-200 text-xs"
          aria-label="Clear pinned focus"
          title="Clear pinned focus (Esc)"
        >
          clear
        </button>
      </div>
      <div className="flex-1 min-h-0">
        <DetailFor for={pinned} />
      </div>
    </div>
  )
}
