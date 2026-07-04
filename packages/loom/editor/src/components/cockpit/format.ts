//! Pure formatting helpers for the Run (Operate) cockpit. Kept out of the
//! `.tsx` component module so Fast Refresh stays happy (a component file should
//! export only components).

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

/** A glyph for a channel kind, so the rooms list scans quickly. */
export function channelGlyph(kind: string | undefined): string {
  switch (kind) {
    case 'lobby':
      return '🌐'
    case 'faction':
      return '🚩'
    case 'location':
      return '📍'
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
