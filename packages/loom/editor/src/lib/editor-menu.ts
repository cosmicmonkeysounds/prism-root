// The text editor's contextual menu + beat-rename command — the CM-side
// twin of the story canvas's right-click menus. Right-click brings up
// the IDE actions for the token/beat under the pointer (go to
// definition, references, rename, reveal on the canvas) plus the
// clipboard basics; Shift+right-click keeps the native browser menu.
// `F2` renames the beat at the cursor through the same workspace-wide
// rename the canvas uses (declaration + every reference + `entry:`).

import { EditorSelection } from '@codemirror/state'
import type { Extension } from '@codemirror/state'
import { EditorView, keymap } from '@codemirror/view'
import { selectAll } from '@codemirror/commands'
import { EditError } from '@loom/core/parser'
import type { StoryGraph } from '@loom/core/lsp'
import { lspWorkspaceSync, uriFor } from '@/lib/lsp-client'
import { syncBuffer } from '@/lib/lsp-index'
import { definitionAtPos, findReferences, wordAt } from '@/lib/loom-lsp'
import { navigateToLocation } from '@/lib/lsp-nav'
import { applyEditMap } from '@/lib/story-graph'
import { nodeAtLine } from '@/components/graph/follow'
import { openContextMenu, type ContextMenuEntry } from '@/store/context-menu'
import { useGraph } from '@/store/graph'

/** The current project graph, with this buffer synced in first. */
function freshGraph(view: EditorView, path: string): StoryGraph {
  syncBuffer(path, view.state.doc.toString())
  return lspWorkspaceSync().storyGraph()
}

/**
 * The beat the cursor addresses: an exact key under the cursor (a
 * divert target / declaration name, `Owner.name` qualified included),
 * else the beat whose block encloses the cursor line.
 */
export function beatKeyAtCursor(view: EditorView, path: string): string | null {
  const graph = freshGraph(view, path)
  const head = view.state.selection.main.head
  const w = wordAt(view.state, head)
  if (w !== null) {
    // Qualified `Owner.name` — the cursor may sit on either segment.
    const before = w.from > 0 ? view.state.sliceDoc(w.from - 1, w.from) : ''
    if (before === '.') {
      const owner = wordAt(view.state, w.from - 1)
      if (owner !== null && graph.beats.has(`${owner.text}.${w.text}`)) {
        return `${owner.text}.${w.text}`
      }
    }
    if (graph.beats.has(w.text)) return w.text
  }
  const line = view.state.doc.lineAt(head).number - 1
  const id = nodeAtLine(graph, uriFor(path), line)
  return id !== null && graph.beats.has(id) ? id : null
}

/** The canvas node (beat or entity) the cursor sits in, for reveal. */
function canvasNodeAtCursor(view: EditorView, path: string): string | null {
  const graph = freshGraph(view, path)
  const line = view.state.doc.lineAt(view.state.selection.main.head).number - 1
  return nodeAtLine(graph, uriFor(path), line)
}

/** Rename the beat at the cursor, workspace-wide. True = handled. */
export async function renameBeatAtCursor(view: EditorView, path: string): Promise<boolean> {
  const key = beatKeyAtCursor(view, path)
  if (key === null) return false
  const beat = lspWorkspaceSync().storyGraph().beats.get(key)
  if (beat === undefined) return false
  if (beat.structural === 'derived') {
    window.alert('Derived beats are template instances — rename the trait’s beat.')
    return true
  }
  const next = window.prompt(`Rename \`${key}\` to:`, beat.name)
  if (next === null || next.trim().length === 0 || next.trim() === beat.name) return true
  try {
    const edits = lspWorkspaceSync().renameBeat(key, next.trim())
    await applyEditMap(edits, `Rename ${key} → ${next.trim()}`)
  } catch (e) {
    window.alert(e instanceof EditError ? e.message : 'Rename failed.')
  }
  return true
}

/** Select + center the enclosing beat/entity on the story canvas.
 *  (`reveal` re-opens a ⌘\-hidden graph pane itself.) */
function revealOnCanvas(id: string): void {
  useGraph.getState().reveal(id)
}

// ---------------------------------------------------------------------------
// Clipboard (CM has no built-in commands for these — they're DOM-level)
// ---------------------------------------------------------------------------

function copySelection(view: EditorView): void {
  const { from, to } = view.state.selection.main
  if (from === to) return
  void navigator.clipboard?.writeText(view.state.sliceDoc(from, to))
}

function cutSelection(view: EditorView): void {
  const { from, to } = view.state.selection.main
  if (from === to) return
  copySelection(view)
  view.dispatch({ changes: { from, to, insert: '' }, selection: EditorSelection.cursor(from) })
  view.focus()
}

function pasteClipboard(view: EditorView): void {
  void navigator.clipboard?.readText().then((text) => {
    if (text.length === 0) return
    view.dispatch(view.state.replaceSelection(text))
    view.focus()
  })
}

// ---------------------------------------------------------------------------
// The extension
// ---------------------------------------------------------------------------

function buildItems(view: EditorView, path: string, lsp: boolean): ContextMenuEntry[] {
  const items: ContextMenuEntry[] = []
  const head = view.state.selection.main.head
  if (lsp) {
    const def = definitionAtPos(view, path, head)
    const beatKey = beatKeyAtCursor(view, path)
    const nodeId = canvasNodeAtCursor(view, path)
    items.push(
      {
        label: 'Go to definition',
        hint: 'F12',
        disabled: def === null,
        testid: 'editor-menu-goto-def',
        onSelect: () => {
          if (def !== null) void navigateToLocation(def)
        },
      },
      {
        label: 'Find references',
        hint: '⇧F12',
        testid: 'editor-menu-references',
        onSelect: () => void findReferences(view, path),
      },
      {
        label: beatKey !== null ? `Rename \`${beatKey}\`…` : 'Rename beat…',
        hint: 'F2',
        disabled: beatKey === null,
        testid: 'editor-menu-rename',
        onSelect: () => void renameBeatAtCursor(view, path),
      },
      {
        label: 'Reveal in story graph',
        disabled: nodeId === null,
        testid: 'editor-menu-reveal',
        onSelect: () => {
          if (nodeId !== null) revealOnCanvas(nodeId)
        },
      },
      { separator: true },
    )
  }
  const hasSelection = !view.state.selection.main.empty
  items.push(
    { label: 'Cut', hint: '⌘X', disabled: !hasSelection, onSelect: () => cutSelection(view) },
    { label: 'Copy', hint: '⌘C', disabled: !hasSelection, onSelect: () => copySelection(view) },
    { label: 'Paste', hint: '⌘V', onSelect: () => pasteClipboard(view) },
    { separator: true },
    {
      label: 'Select all',
      hint: '⌘A',
      onSelect: () => {
        selectAll(view)
        view.focus()
      },
    },
  )
  return items
}

/**
 * The editor's right-click menu (+ the F2 rename binding when `lsp`).
 * Shift+right-click falls through to the browser's native menu.
 */
export function editorContextMenu(path: string, opts: { lsp: boolean }): Extension {
  const ext: Extension[] = [
    EditorView.domEventHandlers({
      contextmenu(event, view) {
        if (event.shiftKey) return false // escape hatch: native menu
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY })
        if (pos === null) return false
        event.preventDefault()
        // Native feel: clicking outside the selection moves the cursor there.
        const sel = view.state.selection.main
        if (pos < sel.from || pos > sel.to) {
          view.dispatch({ selection: EditorSelection.cursor(pos) })
        }
        openContextMenu(buildItems(view, path, opts.lsp), { x: event.clientX, y: event.clientY })
        return true
      },
    }),
  ]
  if (opts.lsp) {
    ext.push(
      keymap.of([
        {
          key: 'F2',
          preventDefault: true,
          run(view) {
            void renameBeatAtCursor(view, path)
            return true
          },
        },
      ]),
    )
  }
  return ext
}
