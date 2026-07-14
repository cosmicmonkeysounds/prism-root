// Cursor-follow resolution: which canvas node encloses a text position?
//
// `GraphBeat.span` / `GraphEntity.span` anchor the *declaration line*
// only (bodies are indent-delimited, so no end offset is indexed).
// But declarations partition a `.loom` file top-to-bottom: everything
// between one section start and the next belongs to it. So the node at
// a line is simply the nearest section start at-or-above it — across
// beats AND entities together, so a cursor inside `CHARACTER X` (after
// an earlier beat) resolves to the character, and a cursor inside an
// owned `beat` block (which starts after its owner's line) resolves to
// the owned beat.

import type { StoryGraph } from '@loom/core/lsp'

type Section = { id: string; line: number }

/**
 * The canvas node id (beat key / entity id) enclosing `line` (0-based)
 * of `uri`, or null when the line precedes every declaration (file
 * header) or the file has none.
 */
export function nodeAtLine(graph: StoryGraph, uri: string, line: number): string | null {
  const sections: Section[] = []
  for (const beat of graph.beats.values()) {
    // Derived beats anchor to their template file — following the cursor
    // there would yank selection to another deriver's instance; skip.
    if (beat.structural === 'derived') continue
    if (beat.uri === uri && beat.span !== null) {
      sections.push({ id: beat.key, line: beat.span.start.line })
    }
  }
  for (const ent of graph.entities.values()) {
    if (ent.uri === uri) sections.push({ id: ent.id, line: ent.span.start.line })
  }
  let best: Section | null = null
  for (const s of sections) {
    if (s.line > line) continue
    if (best === null || s.line > best.line) best = s
  }
  return best?.id ?? null
}
