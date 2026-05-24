import { useEffect, useMemo, useRef } from 'react'
import CodeMirror, { type ReactCodeMirrorRef } from '@uiw/react-codemirror'
import { oneDark } from '@codemirror/theme-one-dark'
import { EditorView, keymap } from '@codemirror/view'
import { EditorState, EditorSelection, type Extension } from '@codemirror/state'
import { indentUnit } from '@codemirror/language'
import { indentWithTab } from '@codemirror/commands'
import { indentationMarkers } from '@replit/codemirror-indentation-markers'
import { useWorkspace } from '@/store/workspace'
import { useSettings } from '@/store/settings'
import { extensionForPath } from '@/lib/language'

export function Editor() {
  const activePath = useWorkspace((s) => s.activePath)
  const file = useWorkspace((s) => (activePath ? s.openFiles[activePath] : null))
  const updateContents = useWorkspace((s) => s.updateContents)
  const saveActive = useWorkspace((s) => s.saveActive)
  const saveAll = useWorkspace((s) => s.saveAll)
  const setCursor = useWorkspace((s) => s.setCursor)
  const pendingCursor = useWorkspace((s) => s.pendingCursor)
  const settings = useSettings()
  const cmRef = useRef<ReactCodeMirrorRef>(null)

  // Save shortcut (Cmd/Ctrl+S; Shift to save all).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
        e.preventDefault()
        if (e.shiftKey) void saveAll()
        else void saveActive()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [saveActive, saveAll])

  // Auto-save: debounce-write dirty active file.
  useEffect(() => {
    if (!settings.autoSave || !file || !file.dirty) return
    const timer = setTimeout(() => void saveActive(), settings.autoSaveDelayMs)
    return () => clearTimeout(timer)
  }, [settings.autoSave, settings.autoSaveDelayMs, file?.dirty, file?.contents, saveActive, file])

  const extensions = useMemo(() => {
    if (!file) return []
    const ext: Extension[] = [...extensionForPath(file.path)]
    ext.push(EditorState.tabSize.of(settings.tabSize))
    ext.push(indentUnit.of(settings.indentWithTabs ? '\t' : ' '.repeat(settings.tabSize)))
    ext.push(keymap.of([indentWithTab]))
    if (settings.wordWrap) ext.push(EditorView.lineWrapping)
    if (settings.showIndentGuides) ext.push(indentationMarkers({ highlightActiveBlock: true, hideFirstIndent: false }))
    ext.push(
      EditorView.theme({
        '&': { fontSize: `${settings.fontSize}px` },
        '.cm-scroller': { fontFamily: settings.fontFamily },
      }),
    )
    ext.push(
      EditorView.updateListener.of((update) => {
        if (!update.selectionSet && !update.docChanged) return
        const sel = update.state.selection.main
        const line = update.state.doc.lineAt(sel.head)
        setCursor({
          line: line.number,
          column: sel.head - line.from + 1,
          selection: Math.abs(sel.to - sel.from),
        })
      }),
    )
    return ext
  }, [
    file?.path,
    settings.tabSize,
    settings.indentWithTabs,
    settings.wordWrap,
    settings.showIndentGuides,
    settings.fontSize,
    settings.fontFamily,
    setCursor,
  ])

  useEffect(() => {
    if (!file) setCursor(null)
  }, [file, setCursor])

  // Honor pending cursor reveal (e.g. clicking a result in project-wide search).
  useEffect(() => {
    if (!pendingCursor || !file || pendingCursor.path !== file.path) return
    // Wait a frame so the document is mounted, then dispatch a selection + scroll.
    const id = requestAnimationFrame(() => {
      const view = cmRef.current?.view
      if (!view) return
      const lineNo = Math.max(1, Math.min(view.state.doc.lines, pendingCursor.line))
      const lineInfo = view.state.doc.line(lineNo)
      const col = Math.max(0, Math.min(lineInfo.length, pendingCursor.column - 1))
      const pos = lineInfo.from + col
      view.dispatch({
        selection: EditorSelection.cursor(pos),
        effects: EditorView.scrollIntoView(pos, { y: 'center' }),
      })
      view.focus()
    })
    return () => cancelAnimationFrame(id)
  }, [pendingCursor, file?.path, file])

  if (!file) {
    return (
      <div className="h-full grid place-items-center text-zinc-500 text-sm">
        <div className="text-center space-y-2">
          <div>Select a file to open it.</div>
          <div className="text-xs text-zinc-600">
            <kbd className="px-1 py-0.5 rounded bg-white/5 border border-white/10">⌘P</kbd>{' '}
            go to file ·{' '}
            <kbd className="px-1 py-0.5 rounded bg-white/5 border border-white/10">⌘⇧P</kbd>{' '}
            command palette ·{' '}
            <kbd className="px-1 py-0.5 rounded bg-white/5 border border-white/10">⌘,</kbd>{' '}
            settings
          </div>
        </div>
      </div>
    )
  }

  return (
    <CodeMirror
      ref={cmRef}
      value={file.contents}
      theme={settings.theme === 'dark' ? oneDark : 'light'}
      extensions={extensions}
      onChange={(value) => updateContents(file.path, value)}
      basicSetup={{
        lineNumbers: settings.lineNumbers,
        highlightActiveLine: settings.highlightActiveLine,
        highlightActiveLineGutter: settings.highlightActiveLine,
        foldGutter: settings.foldGutter,
        autocompletion: settings.autocompletion,
        bracketMatching: settings.bracketMatching,
        closeBrackets: settings.closeBrackets,
        indentOnInput: true,
        searchKeymap: true,
        highlightSelectionMatches: true,
        drawSelection: true,
        rectangularSelection: true,
        crosshairCursor: true,
        history: true,
        tabSize: settings.tabSize,
      }}
      height="100%"
      style={{ height: '100%' }}
    />
  )
}
