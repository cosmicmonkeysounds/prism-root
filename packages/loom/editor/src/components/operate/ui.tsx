//! Small shared primitives for the Run (Operate) cockpit — a faction pill,
//! a channel glyph, and a couple of chrome bits so the sidebar / stage /
//! inspector read consistently. Deliberately tiny; anything bigger belongs
//! in its own component.

import type { ReactNode } from 'react'

/** Tailwind tone classes for a faction pill (falls back to a neutral grey). */
export function factionTone(faction: string | null | undefined): string {
  switch (faction) {
    case 'Mods':
      return 'bg-sky-500/15 text-sky-300'
    case 'Chatters':
      return 'bg-pink-500/15 text-pink-300'
    case 'TheAlgorithm':
      return 'bg-violet-500/15 text-violet-300'
    default:
      return 'bg-zinc-700/40 text-zinc-400'
  }
}

/** A small faction badge; renders "unaligned" when there's no faction. */
export function FactionPill({ faction }: { faction: string | null | undefined }) {
  return (
    <span className={`inline-block rounded-full px-2 py-0.5 text-[11px] font-semibold ${factionTone(faction)}`}>
      {faction ?? 'unaligned'}
    </span>
  )
}

/** A glyph for a channel kind, so the rooms list scans quickly. */
export function channelGlyph(kind: string | undefined): string {
  switch (kind) {
    case 'lobby':
      return '🌐'
    case 'faction':
      return '🚩'
    case 'dm':
      return '✉️'
    case 'announcement':
      return '📢'
    case 'private':
    case 'group':
      return '🔒'
    default:
      return '#'
  }
}

export function SectionLabel({ children }: { children: ReactNode }) {
  return (
    <div className="px-3 pt-3 pb-1 text-[10px] uppercase tracking-widest text-zinc-500">{children}</div>
  )
}

/** A compact key/value row used across the world + inspector panels. */
export function KV({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center gap-3 px-3 py-1 text-xs">
      <div className="w-24 shrink-0 truncate text-zinc-500">{label}</div>
      <div className="min-w-0 flex-1 text-zinc-200">{children}</div>
    </div>
  )
}
