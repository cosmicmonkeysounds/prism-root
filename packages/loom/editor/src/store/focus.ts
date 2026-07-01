// The focus + projection bus (Loom IDE redesign §4). Panels read/write
// through this single store so pinning a beat / character / world-key on
// one surface drives the References panel and the detail overlay on
// every other.
//
// The play-coupled cross-panel "dim related envelopes" resolver was
// removed with the runtime panels (editor is authoring-only now); what
// remains is the small focus store + a detail descriptor.

import { create } from 'zustand'

// ---------------------------------------------------------------------------
// Refs
// ---------------------------------------------------------------------------

export type FocusRef =
  | { kind: 'character'; name: string }
  | { kind: 'beat'; name: string }
  | { kind: 'world-key'; key: string }

export type DetailSink = 'panel' | 'popover' | 'modal' | 'side'

export type DetailDescriptor = {
  ref: FocusRef
  sink: DetailSink
  /** Anchor rect for popover sinks. Ignored otherwise. */
  anchor?: { x: number; y: number; width: number; height: number }
}

/** Canonical string form. Two refs are "the same focus" iff their keys match. */
export function refKey(ref: FocusRef | null): string {
  if (!ref) return ''
  switch (ref.kind) {
    case 'character':
      return `char:${ref.name}`
    case 'beat':
      return `beat:${ref.name}`
    case 'world-key':
      return `wkey:${ref.key}`
  }
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

type FocusState = {
  hover: FocusRef | null
  pinned: FocusRef | null
  detail: DetailDescriptor | null
  /** `side`-sink stack — additive, dismissed individually. */
  sideStack: FocusRef[]
  setHover(ref: FocusRef | null): void
  pin(ref: FocusRef | null): void
  openDetail(ref: FocusRef, opts?: { sink?: DetailSink; anchor?: DetailDescriptor['anchor'] }): void
  closeDetail(): void
  closeSideAt(index: number): void
  clearSide(): void
}

export const useFocus = create<FocusState>((set, get) => ({
  hover: null,
  pinned: null,
  detail: null,
  sideStack: [],
  setHover: (ref) => set({ hover: ref }),
  pin: (ref) => set({ pinned: ref }),
  openDetail: (ref, opts) => {
    const sink: DetailSink = opts?.sink ?? 'panel'
    if (sink === 'side') {
      // Additive: append unless already in the stack.
      const existing = get().sideStack
      const exists = existing.some((r) => refKey(r) === refKey(ref))
      const sideStack = exists ? existing : [...existing, ref]
      set({ pinned: ref, sideStack, detail: { ref, sink, anchor: opts?.anchor } })
    } else {
      set({ pinned: ref, detail: { ref, sink, anchor: opts?.anchor } })
    }
  },
  closeDetail: () => set({ detail: null }),
  closeSideAt: (index) =>
    set((s) => ({ sideStack: s.sideStack.filter((_, i) => i !== index) })),
  clearSide: () => set({ sideStack: [] }),
}))

/** Effective focus for highlighting: hover wins, falls back to pinned. */
export function useEffectiveFocus(): FocusRef | null {
  return useFocus((s) => s.hover ?? s.pinned)
}
