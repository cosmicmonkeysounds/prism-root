// Project story-graph access + graph-side edit application.
//
// The graph itself is computed by `@loom/core/lsp` (`Workspace.storyGraph()`)
// over the whole indexed project — every `.loom` file, not just the active
// buffer. This module is the editor-side glue: a React hook that recomputes
// when the LSP index changes, and the write path that applies `TextEdit`
// batches back through the workspace store (opening affected files as tabs,
// marking them dirty, and pushing the fresh text straight into the LSP
// Workspace so the graph re-derives without waiting for a debounce).

import { useMemo } from 'react'
import { EditError, applyEdits, retargetDivert, type TextEdit } from '@loom/core/parser'
import type { GraphBeat, GraphEdge, StoryGraph } from '@loom/core/lsp'
import { docText, lspWorkspaceSync, pathForUri, uriFor } from '@/lib/lsp-client'
import { syncBuffer } from '@/lib/lsp-index'
import { useLspIndexGen } from '@/lib/lsp-index'
import { findFileEntryByPath } from '@/lib/lsp-nav'
import { useWorkspace } from '@/store/workspace'

/** The whole-project story graph, recomputed when the LSP index changes. */
export function useStoryGraph(): StoryGraph {
  const gen = useLspIndexGen((s) => s.gen)
  return useMemo(() => {
    void gen // dependency: any indexed-text change invalidates the graph
    return lspWorkspaceSync().storyGraph()
  }, [gen])
}

/** Stable per-project key for persisted canvas layout. */
export function useProjectKey(): string {
  const projectId = useWorkspace((s) => s.projectId)
  const rootName = useWorkspace((s) => s.root?.name)
  return projectId ?? rootName ?? 'local'
}

/**
 * Write `next` as the full contents of `path`, opening the file as a tab
 * if it isn't one yet (so the change is visible + undoable + saveable),
 * and sync the LSP Workspace immediately so the graph rebuilds now.
 */
export async function writePathContents(path: string, next: string): Promise<void> {
  const ws = useWorkspace.getState()
  if (!ws.openFiles[path]) {
    const entry = ws.root ? findFileEntryByPath(ws.root, path) : null
    if (!entry) throw new Error(`no file entry for ${path}`)
    await ws.openFile(entry)
  }
  useWorkspace.getState().updateContents(path, next)
  syncBuffer(path, next)
}

/**
 * Apply a per-URI `TextEdit` batch (the shape `Workspace.renameBeat`
 * returns). Each document's edits are computed against the text the LSP
 * Workspace currently holds — which IS the live buffer for open files.
 */
export async function applyEditMap(edits: Map<string, TextEdit[]>): Promise<void> {
  for (const [uri, list] of edits) {
    if (list.length === 0) continue
    const text = docText(uri)
    if (text === null) throw new Error(`no indexed text for ${uri}`)
    await writePathContents(pathForUri(uri), applyEdits(text, list))
  }
}

/** Apply edits to a single document identified by URI. */
export async function applyEditsToUri(uri: string, list: TextEdit[]): Promise<void> {
  await applyEditMap(new Map([[uri, list]]))
}

/**
 * The target text to write into a divert that should reach `beat` —
 * `Owner.name` for owned/derived beats, the bare name for top-level
 * beats. Shadowed duplicates are not addressable (returns null).
 */
export function writtenTargetFor(beat: GraphBeat): string | null {
  if (beat.shadowed) return null
  return beat.key
}

/** Convenience: the graph beat's document path (for reveal / editing). */
export function beatPath(beat: GraphBeat): string | null {
  return beat.uri === null ? null : pathForUri(beat.uri)
}

/**
 * Rewire a narrative edge onto a new target beat by rewriting the
 * divert's target text at its exact source range. Returns null on
 * success, or a human-readable reason the rewire was refused.
 */
export async function rewireGraphEdge(
  graph: StoryGraph,
  edge: GraphEdge,
  targetKey: string,
): Promise<string | null> {
  if (edge.targetRange === null) return 'This connection has no editable source anchor.'
  const target = graph.beats.get(targetKey)
  if (target === undefined) return 'Connections land on beats.'
  const written = writtenTargetFor(target)
  if (written === null) return 'That beat is shadowed — rename one of the duplicates first.'
  const { uri, start, end } = edge.targetRange
  const text = docText(uri)
  if (text === null) return `\`${pathForUri(uri)}\` isn’t indexed yet.`
  try {
    const edits = retargetDivert(text, start, end, text.slice(start, end), written)
    if (edits.length > 0) await applyEditsToUri(uri, edits)
    return null
  } catch (e) {
    return e instanceof EditError ? e.message : 'Could not rewire — source changed underneath.'
  }
}

/** uri for a workspace-relative path (re-export for graph components). */
export { pathForUri, uriFor }
