// Phase 3 of the Loom IDE redesign §4.3: the popover / modal / side
// projection sinks. Mounted once at the App root; switches on the
// `detail.sink` in the focus store. The `panel` sink is rendered by
// the dock panel directly so it doesn't appear here.

import { useEffect, useMemo, useRef } from 'react'
import { useFocus } from '@/store/focus'
import { DetailFor } from './registry'

const POPOVER_WIDTH = 340
const POPOVER_MAX_HEIGHT = 420

export function DetailOverlay() {
  const detail = useFocus((s) => s.detail)
  const sideStack = useFocus((s) => s.sideStack)
  const closeDetail = useFocus((s) => s.closeDetail)
  const pin = useFocus((s) => s.pin)

  useEffect(() => {
    if (!detail) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        closeDetail()
        // For popover/modal, Esc also clears the pin so the timeline
        // dim pass releases. The panel/side sinks keep the pin so
        // the dock + side drawer still have content.
        if (detail.sink !== 'panel' && detail.sink !== 'side') pin(null)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [detail, closeDetail, pin])

  return (
    <>
      {sideStack.length > 0 && <SideDrawer />}
      {detail && detail.sink === 'popover' && <Popover />}
      {detail && detail.sink === 'modal' && <Modal />}
    </>
  )
}

// Drawer is mounted whenever `sideStack` is non-empty, independent of
// `detail` — so opening another detail in `panel`/`popover` sink does
// not collapse it.
function SideDrawer() {
  const stack = useFocus((s) => s.sideStack)
  const closeSideAt = useFocus((s) => s.closeSideAt)
  const clearSide = useFocus((s) => s.clearSide)
  if (stack.length === 0) return null
  return (
    <aside
      role="complementary"
      aria-label="Side details"
      style={{ zIndex: 900 }}
      className="fixed right-0 top-9 bottom-6 w-[340px] flex flex-col border-l border-white/10 bg-zinc-950 shadow-2xl"
    >
      <header className="h-7 px-2 flex items-center text-[11px] text-zinc-500 border-b border-white/10 shrink-0">
        <span>Side · {stack.length} pinned</span>
        <button
          type="button"
          onClick={clearSide}
          className="ml-auto hover:text-zinc-200"
          title="Close all side details"
        >
          clear
        </button>
      </header>
      <div className="flex-1 min-h-0 overflow-auto divide-y divide-white/10">
        {stack.map((entry, i) => (
          <div key={i} className="relative">
            <button
              type="button"
              onClick={() => closeSideAt(i)}
              className="absolute top-1 right-2 text-zinc-600 hover:text-rose-400 text-xs"
              aria-label="Dismiss this side entry"
              title="Dismiss"
            >
              ×
            </button>
            <DetailFor for={entry} />
          </div>
        ))}
      </div>
    </aside>
  )
}

function Popover() {
  const detail = useFocus((s) => s.detail)!
  const closeDetail = useFocus((s) => s.closeDetail)
  const ref = useRef<HTMLDivElement | null>(null)
  const pos = useMemo(() => {
    const a = detail.anchor
    if (!a) return { left: 0, top: 0 }
    const vw = window.innerWidth
    const vh = window.innerHeight
    let left = a.x + a.width + 8
    if (left + POPOVER_WIDTH + 8 > vw) left = Math.max(8, a.x - POPOVER_WIDTH - 8)
    let top = a.y
    if (top + POPOVER_MAX_HEIGHT + 8 > vh) top = Math.max(8, vh - POPOVER_MAX_HEIGHT - 8)
    return { left, top }
  }, [detail])

  useEffect(() => {
    const onClick = (e: MouseEvent) => {
      const node = ref.current
      if (node && node.contains(e.target as Node)) return
      // Outside click on a "data-focusable" hands focus to that
      // element instead of dismissing — feels natural when scrubbing.
      const target = e.target as HTMLElement | null
      if (!target?.closest('[data-focusable]')) closeDetail()
    }
    // Defer one tick so the click that opened the popover doesn't
    // immediately close it.
    const id = window.setTimeout(() => window.addEventListener('click', onClick), 0)
    return () => {
      window.clearTimeout(id)
      window.removeEventListener('click', onClick)
    }
  }, [closeDetail])

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label="Focused entity"
      style={{
        position: 'fixed',
        left: pos.left,
        top: pos.top,
        width: POPOVER_WIDTH,
        maxHeight: POPOVER_MAX_HEIGHT,
        zIndex: 1000,
      }}
      className="rounded border border-white/10 shadow-2xl bg-zinc-950 overflow-hidden flex flex-col"
    >
      <div className="flex-1 min-h-0">
        <DetailFor for={detail.ref} />
      </div>
    </div>
  )
}

function Modal() {
  const detail = useFocus((s) => s.detail)!
  const closeDetail = useFocus((s) => s.closeDetail)
  const pin = useFocus((s) => s.pin)
  return (
    <div
      role="dialog"
      aria-modal="true"
      style={{ zIndex: 1000 }}
      className="fixed inset-0 grid place-items-center bg-black/60 px-4"
      onClick={() => {
        closeDetail()
        pin(null)
      }}
    >
      <div
        className="rounded border border-white/10 bg-zinc-950 w-[480px] max-h-[70vh] overflow-hidden flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex-1 min-h-0">
          <DetailFor for={detail.ref} />
        </div>
      </div>
    </div>
  )
}
