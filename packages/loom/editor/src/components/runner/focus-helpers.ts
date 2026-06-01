// Phase 3 helpers — the glue between any panel and the focus +
// projection bus. Three primitives:
//
//   useFocusableProps(ref, sink?)  — pointer handlers + data-attr
//   useDim(predicate)              — Tailwind class when hover is
//                                    active and `predicate(related)`
//                                    is false
//   useIsFocused(ref)              — element matches the effective focus
//
// Panels never reach into the focus store directly; they go through
// these so the projection contract stays in one file.

import { useMemo } from 'react'
import {
  refKey,
  useEffectiveFocus,
  useFocus,
  useRelated,
  type DetailSink,
  type FocusRef,
  type Related,
} from '@/store/focus'

export function useFocusableProps(ref: FocusRef, sink: DetailSink = 'panel') {
  const setHover = useFocus((s) => s.setHover)
  const openDetail = useFocus((s) => s.openDetail)
  // `ref` is recreated each render in callers; key by its canonical
  // form so we don't churn handlers when only the object identity
  // changes.
  const key = refKey(ref)
  return useMemo(
    () => ({
      'data-focusable': true,
      onPointerEnter: () => setHover(ref),
      onPointerLeave: () => setHover(null),
      onClick: (e: React.MouseEvent<HTMLElement>) => {
        const target = e.currentTarget as HTMLElement
        const r = target.getBoundingClientRect()
        // Shift-click adds the entry to the right-edge side drawer
        // instead of opening the default sink — same affordance for
        // every focusable so multi-pin works uniformly across panels.
        // Alt-click forces the modal sink.
        const overrideSink: DetailSink = e.shiftKey
          ? 'side'
          : e.altKey
            ? 'modal'
            : sink
        openDetail(ref, {
          sink: overrideSink,
          anchor: { x: r.left, y: r.top, width: r.width, height: r.height },
        })
        e.stopPropagation()
      },
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [key, sink, setHover, openDetail],
  )
}

/**
 * Tailwind opacity class for the dim pass. Behaves as:
 * - hover active + element NOT in related set → dimmed
 * - hover inactive (or hover hit this element) → no class change
 */
export function useDim(predicate: (related: Related) => boolean): string {
  const hover = useFocus((s) => s.hover)
  const related = useRelated()
  if (!hover) return ''
  return predicate(related) ? '' : 'opacity-30'
}

export function useIsFocused(ref: FocusRef): boolean {
  const focus = useEffectiveFocus()
  return refKey(focus) === refKey(ref)
}
