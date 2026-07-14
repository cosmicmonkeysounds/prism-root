// CodeMirror ⇄ `@loom/core/lsp` glue: hover tooltips, LSP-backed
// completion, ⌘/Ctrl-click + F12 go-to-definition, ⌘/Ctrl-hover "link"
// underline, Shift-F12 find-references, and occurrence highlighting.
//
// Everything here reads LIVE state through getters (the singleton
// `Workspace` + the zustand stores) at event time, so the extension array
// only ever depends on `file.path` + display settings — never on per-keystroke
// store churn. That keeps `@uiw/react-codemirror` from reconfiguring (and
// tearing down these ViewPlugins) on every edit. See the design doc.

import {
  EditorView,
  ViewPlugin,
  Decoration,
  hoverTooltip,
  keymap,
  type DecorationSet,
  type Tooltip,
  type ViewUpdate,
  type KeyBinding,
} from '@codemirror/view'
import { EditorState, type Extension } from '@codemirror/state'
import {
  autocompletion,
  completionKeymap,
  type CompletionSource,
  type Completion,
} from '@codemirror/autocomplete'
import { lspWorkspaceSync, uriFor } from '@/lib/lsp-client'
import { syncBuffer } from '@/lib/lsp-index'
import { offsetToLsp, lspToOffset, firstLocation, navigateToLocation } from '@/lib/lsp-nav'
import { useFocus } from '@/store/focus'

// ── shared helpers ───────────────────────────────────────────────────

/**
 * Sync the current buffer into the Workspace (diffed — a no-op when the
 * debounced project-index effect already pushed it) and return the singleton
 * + this file's URI. Guarantees every query reflects the exact buffer, even
 * a keystroke ahead of the 120ms debounce.
 */
function freshLsp(state: EditorState, path: string): { ws: ReturnType<typeof lspWorkspaceSync>; uri: string } {
  syncBuffer(path, state.doc.toString())
  return { ws: lspWorkspaceSync(), uri: uriFor(path) }
}

const WORD = /[A-Za-z_][\w]*/g

/** The identifier span covering `pos`, or `null`. */
export function wordAt(state: EditorState, pos: number): { from: number; to: number; text: string } | null {
  const line = state.doc.lineAt(pos)
  const text = line.text
  WORD.lastIndex = 0
  for (let m = WORD.exec(text); m; m = WORD.exec(text)) {
    const from = line.from + m.index
    const to = from + m[0].length
    if (pos >= from && pos <= to) return { from, to, text: m[0] }
  }
  return null
}

// ── §1 hover ─────────────────────────────────────────────────────────

function loomHover(path: string, delayMs: number): Extension {
  return hoverTooltip(
    (view, pos): Tooltip | null => {
      const { ws, uri } = freshLsp(view.state, path)
      const hover = ws.hoverAt(uri, offsetToLsp(view.state, pos))
      if (!hover) return null
      let from = pos
      let to = pos
      if (hover.range) {
        from = lspToOffset(view.state, hover.range.start)
        to = lspToOffset(view.state, hover.range.end)
      } else {
        const w = wordAt(view.state, pos)
        if (w) {
          from = w.from
          to = w.to
        }
      }
      return {
        pos: from,
        end: to,
        above: true,
        create() {
          const dom = document.createElement('div')
          dom.className = 'cm-loom-hover'
          renderMarkdownInto(dom, hover.contents.value)
          return { dom }
        },
      }
    },
    { hoverTime: delayMs },
  )
}

/**
 * Minimal XSS-safe markdown → DOM for the subset `core/src/lsp/hover.ts`
 * emits: **bold**, inline `code`, ```loom fenced blocks```, and \n\n
 * paragraphs. Every text node is `textContent` — never `innerHTML` with the
 * (user-authored) value.
 */
function renderMarkdownInto(root: HTMLElement, md: string): void {
  for (const block of md.split(/\n{2,}/)) {
    const fence = block.match(/^```(\w*)\n([\s\S]*?)\n?```$/)
    if (fence) {
      const pre = document.createElement('pre')
      pre.className = 'cm-loom-hover-code'
      const code = document.createElement('code')
      code.textContent = fence[2]
      pre.appendChild(code)
      root.appendChild(pre)
      continue
    }
    const p = document.createElement('p')
    renderInline(p, block)
    root.appendChild(p)
  }
}

function renderInline(parent: HTMLElement, text: string): void {
  const re = /(\*\*[^*]+\*\*|`[^`]+`)/g
  let last = 0
  for (const m of text.matchAll(re)) {
    const i = m.index ?? 0
    if (i > last) parent.appendChild(document.createTextNode(text.slice(last, i)))
    const tok = m[0]
    if (tok.startsWith('**')) {
      const b = document.createElement('strong')
      b.textContent = tok.slice(2, -2)
      parent.appendChild(b)
    } else {
      const c = document.createElement('code')
      c.textContent = tok.slice(1, -1)
      parent.appendChild(c)
    }
    last = i + tok.length
  }
  if (last < text.length) parent.appendChild(document.createTextNode(text.slice(last)))
}

// ── §2 completion ────────────────────────────────────────────────────

// core/src/lsp/types.ts CompletionItemKind → CM Completion.type (icon set).
const KIND_TO_TYPE: Record<number, string> = {
  3: 'function', // Function — beats / divert targets
  7: 'class', //   Class — CHARACTER / ROLE
  8: 'interface', // Interface — TRAIT
  14: 'keyword', // Keyword — directives / mixin words
}

function loomCompletionSource(path: string): CompletionSource {
  return (ctx) => {
    // Replace only the trailing IDENTIFIER segment. `completionAt` detects the
    // context (divert / owner-qualified divert `-> self.rep` / directive /
    // `is Trait(` …) from the whole line prefix and returns BARE names, so a
    // dotted/slashed prefix like `-> self.rep` must keep its `self.` and swap
    // only `rep` — matching the greedy `[\w./|#-]*` here would clobber the
    // qualifier and wreck the fuzzy filter.
    const token = ctx.matchBefore(/\w*/)
    if (!token || (token.from === token.to && !ctx.explicit)) return null

    const { ws, uri } = freshLsp(ctx.state, path)
    const items = ws.completionAt(uri, offsetToLsp(ctx.state, ctx.pos))
    if (!items || items.length === 0) return null

    const options: Completion[] = items.map((it) => ({
      label: it.label,
      type: it.kind != null ? (KIND_TO_TYPE[it.kind] ?? 'text') : 'text',
    }))
    return {
      from: token.from,
      options,
      // Keep filtering in-memory while the user types word chars; a `.` / `/`
      // ends the segment and re-queries (the context may have changed).
      validFor: /^\w*$/,
    }
  }
}

// ── §3 go-to-definition (click + keymap + mod-hover underline) ────────

export function definitionAtPos(view: EditorView, path: string, pos: number) {
  const { ws, uri } = freshLsp(view.state, path)
  return firstLocation(ws.definitionAt(uri, offsetToLsp(view.state, pos)))
}

function gotoKeymap(path: string): Extension {
  const defBinding: KeyBinding = {
    key: 'F12',
    preventDefault: true,
    run(view) {
      const loc = definitionAtPos(view, path, view.state.selection.main.head)
      if (!loc) return false
      void navigateToLocation(loc)
      return true
    },
  }
  const refBinding: KeyBinding = {
    key: 'Shift-F12',
    preventDefault: true,
    run(view) {
      return findReferences(view, path)
    },
  }
  return keymap.of([defBinding, refBinding])
}

/** Pin the symbol under the cursor into the focus bus → References panel. */
export function findReferences(view: EditorView, path: string): boolean {
  const head = view.state.selection.main.head
  const w = wordAt(view.state, head)
  if (!w) return false
  // Sync the buffer first so `referencesAt` resolves against current text.
  freshLsp(view.state, path)
  useFocus.getState().pin({
    kind: 'symbol',
    label: w.text,
    uri: uriFor(path),
    pos: offsetToLsp(view.state, head),
  })
  return true
}

function modClickGoto(path: string): Extension {
  return EditorView.domEventHandlers({
    mousedown(event, view) {
      if (!(event.metaKey || event.ctrlKey) || event.button !== 0) return false
      const pos = view.posAtCoords({ x: event.clientX, y: event.clientY })
      if (pos == null) return false
      const loc = definitionAtPos(view, path, pos)
      if (!loc) return false
      event.preventDefault()
      void navigateToLocation(loc)
      return true
    },
  })
}

const linkMark = Decoration.mark({ class: 'cm-loom-gotolink' })

/** While ⌘/Ctrl is held, underline the hovered token when it has a definition. */
function modHoverUnderline(path: string): Extension {
  return ViewPlugin.fromClass(
    class {
      deco: DecorationSet = Decoration.none
      markedFrom = -1
      markedTo = -1
      // Last token span we ran `definitionAt` on + whether it HAD a def —
      // cached so re-hovering the same token neither re-queries the LSP nor
      // fails to re-apply the underline (a miss short-circuits too).
      queriedFrom = -1
      queriedTo = -1
      queriedHadDef = false
      view: EditorView
      private readonly onKeyUp = () => this.clear()

      constructor(view: EditorView) {
        this.view = view
        window.addEventListener('keyup', this.onKeyUp)
        window.addEventListener('blur', this.onKeyUp)
      }
      destroy() {
        window.removeEventListener('keyup', this.onKeyUp)
        window.removeEventListener('blur', this.onKeyUp)
      }
      update(u: ViewUpdate) {
        // Dispatching is forbidden mid-update; just reset the decoration state.
        // The `decorations` accessor is re-read as part of this same update.
        if (u.docChanged) {
          // A definition result is a function of buffer content, so an edit
          // invalidates the cached query span regardless of marked state.
          this.queriedFrom = -1
          this.queriedTo = -1
          if (this.markedFrom >= 0) this.reset()
        }
      }
      /** Reset marked state without forcing a re-render (update-safe). */
      reset() {
        this.markedFrom = -1
        this.markedTo = -1
        this.deco = Decoration.none
      }
      /** Reset AND force a re-render — for DOM-event callers (outside update). */
      clear() {
        if (this.markedFrom < 0) return
        this.reset()
        this.view.dispatch({})
      }
      /** Underline `w` (idempotent — no re-render if already shown there). */
      private mark(w: { from: number; to: number }) {
        if (this.markedFrom === w.from && this.markedTo === w.to) return
        this.markedFrom = w.from
        this.markedTo = w.to
        this.deco = Decoration.set([linkMark.range(w.from, w.to)])
        this.view.dispatch({})
      }
      handleMove(event: MouseEvent) {
        if (!(event.metaKey || event.ctrlKey)) {
          this.clear()
          return
        }
        const pos = this.view.posAtCoords({ x: event.clientX, y: event.clientY })
        if (pos == null) {
          this.clear()
          return
        }
        const w = wordAt(this.view.state, pos)
        if (!w) {
          this.clear()
          return
        }
        // Same token as last query — re-apply the cached answer without hitting
        // the LSP again (bounds queries to token transitions; buffer edits
        // invalidate the cache in `update`).
        if (w.from === this.queriedFrom && w.to === this.queriedTo) {
          if (this.queriedHadDef) this.mark(w)
          else this.clear()
          return
        }
        this.queriedFrom = w.from
        this.queriedTo = w.to
        const loc = definitionAtPos(this.view, path, pos)
        this.queriedHadDef = !!loc
        if (!loc) {
          this.clear()
          return
        }
        this.mark(w)
      }
    },
    {
      decorations: (v) => v.deco,
      eventHandlers: {
        mousemove(event) {
          ;(this as { handleMove(e: MouseEvent): void }).handleMove(event)
        },
        mouseleave() {
          ;(this as { clear(): void }).clear()
        },
      },
    },
  )
}

// ── §4 occurrence highlight ──────────────────────────────────────────

const occMark = Decoration.mark({ class: 'cm-loom-occurrence' })

const occurrenceHighlighter: Extension = ViewPlugin.fromClass(
  class {
    deco: DecorationSet
    constructor(view: EditorView) {
      this.deco = this.compute(view)
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.selectionSet || u.viewportChanged) this.deco = this.compute(u.view)
    }
    compute(view: EditorView): DecorationSet {
      const sel = view.state.selection.main
      if (!sel.empty) return Decoration.none // selection-match handles ranges
      const word = wordAt(view.state, sel.head)
      if (!word || word.text.length < 2) return Decoration.none
      const ranges: Array<ReturnType<typeof occMark.range>> = []
      for (const { from, to } of view.visibleRanges) {
        const text = view.state.doc.sliceString(from, to)
        WORD.lastIndex = 0
        for (let m = WORD.exec(text); m; m = WORD.exec(text)) {
          if (m[0] !== word.text) continue
          const start = from + m.index
          ranges.push(occMark.range(start, start + m[0].length))
        }
      }
      // Ranges are collected left-to-right per visible range and the ranges
      // themselves are ordered, so this is already sorted — but pass sort just
      // in case a future multi-range viewport interleaves.
      return Decoration.set(ranges, true)
    }
  },
  { decorations: (v) => v.deco },
)

// ── theme ────────────────────────────────────────────────────────────

const loomBaseTheme = EditorView.baseTheme({
  '.cm-loom-hover': {
    maxWidth: '460px',
    maxHeight: '340px',
    overflow: 'auto',
    padding: '6px 10px',
    fontSize: '12px',
    lineHeight: '1.5',
  },
  '.cm-loom-hover p': { margin: '0 0 4px' },
  '.cm-loom-hover p:last-child': { margin: '0' },
  '.cm-loom-hover code': {
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
    fontSize: '11px',
    padding: '0 2px',
    borderRadius: '3px',
    background: 'rgba(127,127,127,0.18)',
  },
  '.cm-loom-hover-code': {
    margin: '4px 0',
    padding: '6px 8px',
    borderRadius: '4px',
    background: 'rgba(127,127,127,0.12)',
    overflow: 'auto',
  },
  '.cm-loom-hover-code code': { background: 'none', padding: '0' },
  '.cm-loom-gotolink': { textDecoration: 'underline', cursor: 'pointer' },
  '.cm-loom-occurrence': {
    backgroundColor: 'rgba(120,170,255,0.18)',
    borderRadius: '2px',
  },
})

// ── public factory ───────────────────────────────────────────────────

export interface LoomLspOptions {
  hoverEnabled: boolean
  hoverDelayMs: number
  lspCompletion: boolean
  gotoOnClick: boolean
  occurrenceHighlight: boolean
}

/**
 * Build the CodeMirror IDE extensions for a `.loom` file. Call from the
 * editor's extension memo, gated on the active file being `.loom`. Each
 * factory closes over `path` (stable for the tab's life) and reads live
 * Workspace/store state at event time.
 */
export function loomLspExtensions(path: string, opts: LoomLspOptions): Extension[] {
  const ext: Extension[] = [loomBaseTheme, gotoKeymap(path)]
  if (opts.hoverEnabled) ext.push(loomHover(path, opts.hoverDelayMs))
  if (opts.lspCompletion) {
    ext.push(
      autocompletion({ override: [loomCompletionSource(path)], activateOnTyping: true }),
      // Ensure Ctrl-Space / Enter / arrows drive the menu even when the
      // generic word-completer (basicSetup.autocompletion) is turned off.
      keymap.of(completionKeymap),
    )
  }
  if (opts.gotoOnClick) ext.push(modClickGoto(path), modHoverUnderline(path))
  if (opts.occurrenceHighlight) ext.push(occurrenceHighlighter)
  return ext
}
