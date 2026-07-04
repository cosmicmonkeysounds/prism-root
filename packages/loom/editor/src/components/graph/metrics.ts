// Sizing + glyph tables shared by the graph node components, the flow
// builders, and the ELK layout pass — kept out of the component files
// so react-refresh stays happy and the two sides can never disagree
// about a node's box.

import type { GraphBeat } from '@loom/core/lsp'

// ---- project-view nodes ----------------------------------------------------

export const BEAT_W = 216
export const ENTITY_W = 148
export const ENTITY_H = 30
export const GHOST_W = 168
export const GHOST_H = 42
export const END_W = 88
export const END_H = 34

/** Collapsed file containers render as a fixed header-only card. */
export const FILE_COLLAPSED_W = 220
export const FILE_COLLAPSED_H = 40

/** Expanded beat cards get a wider column so word blocks read well. */
export const BEAT_EXPANDED_W = 280
/** Word-block sizing shared with `word-blocks.ts`'s height estimate. */
export const BODY_BLOCK_LINE_H = 14
export const BODY_BLOCK_CHARS_PER_LINE = 36

export function beatNodeSize(
  beat: GraphBeat,
  expandedBlocksHeight: number | null = null,
): { width: number; height: number } {
  let h = 40 // header
  if (beat.owner !== null || beat.cast.length > 0 || beat.setting !== null) h += 16
  if (expandedBlocksHeight !== null) {
    // Expanded: the full word-block list replaces the preview.
    h += expandedBlocksHeight + 8
  } else {
    h += Math.min(beat.preview.length, 3) * 15
  }
  h += 20 // counts strip
  return { width: expandedBlocksHeight !== null ? BEAT_EXPANDED_W : BEAT_W, height: h + 12 }
}

const ENTITY_GLYPH: Record<string, { glyph: string; cls: string }> = {
  character: { glyph: '◉', cls: 'text-cyan-300 border-cyan-400/40' },
  role: { glyph: '◎', cls: 'text-cyan-200 border-cyan-300/40' },
  trait: { glyph: '◈', cls: 'text-fuchsia-300 border-fuchsia-400/40' },
  location: { glyph: '▦', cls: 'text-amber-300 border-amber-400/40' },
  faction: { glyph: '⚑', cls: 'text-rose-300 border-rose-400/40' },
  item: { glyph: '◆', cls: 'text-lime-300 border-lime-400/40' },
  cohort: { glyph: '❖', cls: 'text-purple-300 border-purple-400/40' },
  space: { glyph: '▣', cls: 'text-sky-300 border-sky-400/40' },
  channel: { glyph: '#', cls: 'text-sky-200 border-sky-300/40' },
}

export function entityGlyph(kind: string): { glyph: string; cls: string } {
  return ENTITY_GLYPH[kind] ?? { glyph: '·', cls: 'text-zinc-400 border-zinc-500/40' }
}

// ---- beat drill-in nodes ---------------------------------------------------

export const BODY_W = 240
export const EXIT_W = 168
export const EXIT_H = 34
export const BRANCH_W = 150
export const BRANCH_H = 34
export const START_W = 260

function lineCount(s: string): number {
  return Math.max(1, Math.ceil(s.length / 34))
}

export function bodyTextSize(text: string): { width: number; height: number } {
  return { width: BODY_W, height: 22 + lineCount(text) * 14 }
}
export function bodyDialogueSize(lines: string[]): { width: number; height: number } {
  const content = lines.reduce((n, l) => n + lineCount(l), 0)
  return { width: BODY_W, height: 30 + Math.max(1, content) * 14 }
}
export function bodyChoiceSize(text: string): { width: number; height: number } {
  return { width: BODY_W, height: 26 + lineCount(text) * 14 }
}
