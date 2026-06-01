// Tiny context-menu primitive. Used by Phase 4 Timeline to host the
// "Fork from here" / "Snapshot here" actions on right-click of a
// ledger envelope, and reusable by any other panel that wants a
// pointer-anchored action list.
//
// State + opener (`useContextMenu`, `openContextMenu`) live in
// `@/store/context-menu` so this file is component-only.

import { useEffect, useLayoutEffect, useRef } from 'react'
import { useContextMenu } from '@/store/context-menu'

export function ContextMenuHost() {
  const items = useContextMenu((s) => s.items)
  const anchor = useContextMenu((s) => s.anchor)
  const close = useContextMenu((s) => s.close)
  const ref = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!items) return
    const onClick = (e: MouseEvent) => {
      if (ref.current?.contains(e.target as Node)) return
      close()
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close()
    }
    const id = window.setTimeout(() => {
      window.addEventListener('click', onClick)
      window.addEventListener('keydown', onKey)
    }, 0)
    return () => {
      window.clearTimeout(id)
      window.removeEventListener('click', onClick)
      window.removeEventListener('keydown', onKey)
    }
  }, [items, close])

  useLayoutEffect(() => {
    if (!ref.current || !anchor) return
    const r = ref.current.getBoundingClientRect()
    const vw = window.innerWidth
    const vh = window.innerHeight
    let left = anchor.x
    let top = anchor.y
    if (left + r.width + 8 > vw) left = Math.max(8, vw - r.width - 8)
    if (top + r.height + 8 > vh) top = Math.max(8, vh - r.height - 8)
    ref.current.style.left = `${left}px`
    ref.current.style.top = `${top}px`
  }, [anchor, items])

  if (!items || !anchor) return null

  return (
    <div
      ref={ref}
      role="menu"
      style={{ position: 'fixed', zIndex: 2000 }}
      className="min-w-[180px] rounded border border-white/10 bg-zinc-950 shadow-2xl text-xs py-1 font-mono"
    >
      {items.map((item, i) => (
        <button
          key={i}
          type="button"
          role="menuitem"
          disabled={item.disabled}
          onClick={() => {
            if (item.disabled) return
            item.onSelect()
            close()
          }}
          className={
            'block w-full text-left px-3 py-1 ' +
            (item.disabled
              ? 'text-zinc-700 cursor-not-allowed'
              : item.kind === 'danger'
                ? 'text-rose-300 hover:bg-rose-400/10'
                : 'text-zinc-200 hover:bg-blue-400/10')
          }
        >
          {item.label}
        </button>
      ))}
    </div>
  )
}
