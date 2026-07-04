//! Small shared components for the Run (Operate) cockpit. Pure formatting
//! helpers live alongside in `format.ts` (kept separate so this component file
//! exports only components).

import { factionTone } from './format'

/** A small faction badge; renders "unaligned" when there's no faction. */
export function FactionPill({ faction }: { faction: string | null | undefined }) {
  return (
    <span className={`inline-block rounded-full px-2 py-0.5 text-[11px] font-semibold ${factionTone(faction)}`}>
      {faction ?? 'unaligned'}
    </span>
  )
}
