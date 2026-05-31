// Phase 3 of the Loom IDE redesign (docs/dev/loom-ide-redesign.md §4):
// the focus + projection bus. Every panel reads/writes through this
// single store so hover on one surface dims unrelated elements on
// every other surface, and click pins focus + opens a detail in the
// configured sink (panel / popover / modal / side).
//
// The store stays deliberately small: hover, pinned, and an open
// detail descriptor. Resolution (what counts as "related" to a focus
// ref) lives in `useRelated` below — a pure function of (ref, play
// snapshot) so panels can subscribe with normal selector semantics.

import { create } from 'zustand'
import { useSession } from './session'
import {
  primaryHead,
  type PlayHeadState,
  type PlayEnvelopeMeta,
  type PlayTrackInfo,
} from '@/lib/sync'
import { eventTag, type LedgerEvent } from '@/components/runner/event-format'

// ---------------------------------------------------------------------------
// Refs
// ---------------------------------------------------------------------------

export type FocusRef =
  | { kind: 'envelope'; head: string; idx: number }
  | { kind: 'character'; name: string }
  | { kind: 'beat'; name: string }
  | { kind: 'track'; head: string; track: number }
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
    case 'envelope':
      return `env:${ref.head}:${ref.idx}`
    case 'character':
      return `char:${ref.name}`
    case 'beat':
      return `beat:${ref.name}`
    case 'track':
      return `track:${ref.head}:${ref.track}`
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
  setHover(ref: FocusRef | null): void
  pin(ref: FocusRef | null): void
  openDetail(ref: FocusRef, opts?: { sink?: DetailSink; anchor?: DetailDescriptor['anchor'] }): void
  closeDetail(): void
}

export const useFocus = create<FocusState>((set) => ({
  hover: null,
  pinned: null,
  detail: null,
  setHover: (ref) => set({ hover: ref }),
  pin: (ref) => set({ pinned: ref }),
  openDetail: (ref, opts) =>
    set({
      pinned: ref,
      detail: { ref, sink: opts?.sink ?? 'panel', anchor: opts?.anchor },
    }),
  closeDetail: () => set({ detail: null }),
}))

/** Effective focus for highlighting: hover wins, falls back to pinned. */
export function useEffectiveFocus(): FocusRef | null {
  return useFocus((s) => s.hover ?? s.pinned)
}

// ---------------------------------------------------------------------------
// Resolver — useRelated
// ---------------------------------------------------------------------------

export type Related = {
  envelopes: Set<number>
  tracks: Set<number>
  characters: Set<string>
  worldKeys: Set<string>
  beats: Set<string>
}

const EMPTY_RELATED: Related = {
  envelopes: new Set(),
  tracks: new Set(),
  characters: new Set(),
  worldKeys: new Set(),
  beats: new Set(),
}

/** Per-character names that the runtime mentions in an envelope body. */
function charactersIn(event: LedgerEvent): string[] {
  const [tag, body] = eventTag(event)
  switch (tag) {
    case 'Dialogue': {
      const speakers = (body.speakers as string[]) ?? [body.speaker as string]
      return speakers.filter(Boolean)
    }
    case 'KnowledgeChanged':
      return [String(body.character ?? '')].filter(Boolean)
    case 'CastBound':
    case 'CastReleased':
    case 'CastSwapped':
    case 'RolePromoted':
      return [String(body.role ?? body.person ?? '')].filter(Boolean)
    default:
      return []
  }
}

/** World keys that an envelope writes. */
function worldKeysOf(event: LedgerEvent): string[] {
  const [tag, body] = eventTag(event)
  if (tag === 'WorldSet') return [String(body.key ?? '')].filter(Boolean)
  if (tag === 'KnowledgeChanged') {
    const c = String(body.character ?? '')
    const f = String(body.field ?? '')
    return c && f ? [`${c}.knows.${f}`] : []
  }
  return []
}

/** Beat names mentioned in an envelope (BeatEntered / Diverted / Tunneled). */
function beatsIn(event: LedgerEvent): string[] {
  const [tag, body] = eventTag(event)
  if (tag === 'BeatEntered') return [String(body.beat ?? '')].filter(Boolean)
  if (tag === 'Diverted' || tag === 'Tunneled')
    return [String(body.target ?? body.beat ?? '')].filter(Boolean)
  return []
}

/**
 * Pure resolver — walks the play state and returns the set of
 * everything that should highlight when `ref` is the active focus.
 * Returns `EMPTY_RELATED` when `ref` is null so the dim pass is a
 * no-op outside hover mode.
 */
export function computeRelated(
  ref: FocusRef | null,
  head: PlayHeadState | null,
): Related {
  if (!ref || !head) return EMPTY_RELATED
  const events = head.transcript as LedgerEvent[]
  const meta: PlayEnvelopeMeta[] = head.meta ?? []
  const tracks: PlayTrackInfo[] = head.tracks ?? []
  const out: Related = {
    envelopes: new Set(),
    tracks: new Set(),
    characters: new Set(),
    worldKeys: new Set(),
    beats: new Set(),
  }

  // Helper: include an envelope plus surface its track/characters/etc.
  const includeEnv = (idx: number) => {
    if (idx < 0 || idx >= events.length || out.envelopes.has(idx)) return
    out.envelopes.add(idx)
    const m = meta[idx]
    if (m) out.tracks.add(m.track)
    for (const c of charactersIn(events[idx])) out.characters.add(c)
    for (const w of worldKeysOf(events[idx])) out.worldKeys.add(w)
    for (const b of beatsIn(events[idx])) out.beats.add(b)
  }

  switch (ref.kind) {
    case 'envelope': {
      includeEnv(ref.idx)
      // Walk cause chain backward.
      let cur = meta[ref.idx]?.cause ?? null
      while (cur != null) {
        includeEnv(cur)
        cur = meta[cur]?.cause ?? null
      }
      // Walk descendants forward — envelopes whose cause is in the set.
      for (let i = 0; i < events.length; i++) {
        const c = meta[i]?.cause
        if (c != null && out.envelopes.has(c)) includeEnv(i)
      }
      break
    }
    case 'track': {
      out.tracks.add(ref.track)
      for (let i = 0; i < events.length; i++) {
        if (meta[i]?.track === ref.track) includeEnv(i)
      }
      break
    }
    case 'character': {
      out.characters.add(ref.name)
      const trackId = tracks.find((t) => t.label === ref.name)?.id
      if (trackId != null) out.tracks.add(trackId)
      const prefix = `${ref.name}.`
      for (let i = 0; i < events.length; i++) {
        if (charactersIn(events[i]).includes(ref.name)) includeEnv(i)
        else if (worldKeysOf(events[i]).some((k) => k.startsWith(prefix))) includeEnv(i)
      }
      for (const [k] of head.world ?? []) {
        if (k.startsWith(prefix)) out.worldKeys.add(k)
      }
      break
    }
    case 'world-key': {
      out.worldKeys.add(ref.key)
      for (let i = 0; i < events.length; i++) {
        if (worldKeysOf(events[i]).includes(ref.key)) includeEnv(i)
      }
      break
    }
    case 'beat': {
      out.beats.add(ref.name)
      for (let i = 0; i < events.length; i++) {
        if (beatsIn(events[i]).includes(ref.name)) includeEnv(i)
      }
      break
    }
  }
  return out
}

/**
 * Reactive variant of `computeRelated`. Re-runs when the effective
 * focus or the play snapshot changes; otherwise stable across renders.
 */
export function useRelated(): Related {
  const focus = useEffectiveFocus()
  const head = useSession((s) => primaryHead(s.active?.play ?? null))
  // Compute synchronously — the cost is dominated by ledger length,
  // which is bounded by the play session length and small in practice.
  // If profiling reveals this is hot we can memoise per (focus, head).
  return computeRelated(focus, head)
}
