import { useWorkspace } from '@/store/workspace'
import { useSettings } from '@/store/settings'
import { languageForPath, languageLabel } from '@/lib/language'

export function StatusBar() {
  const activePath = useWorkspace((s) => s.activePath)
  const file = useWorkspace((s) => (activePath ? s.openFiles[activePath] : null))
  const cursor = useWorkspace((s) => s.cursor)
  const tabSize = useSettings((s) => s.tabSize)
  const indentWithTabs = useSettings((s) => s.indentWithTabs)
  const wordWrap = useSettings((s) => s.wordWrap)
  const setSetting = useSettings((s) => s.set)

  const lang = activePath ? languageLabel(languageForPath(activePath)) : null

  return (
    <footer className="h-6 px-3 flex items-center justify-between text-[11px] text-zinc-500 border-t border-white/10 bg-zinc-950 select-none">
      <div className="flex items-center gap-3 truncate">
        <span className="truncate">{activePath ?? 'No file open'}</span>
        {file && (
          <span className={file.dirty ? 'text-amber-400' : 'text-emerald-400'}>
            {file.dirty ? '● Unsaved' : '✓ Saved'}
          </span>
        )}
      </div>
      <div className="flex items-center gap-4">
        {cursor && (
          <span>
            Ln {cursor.line}, Col {cursor.column}
            {cursor.selection > 0 ? ` (${cursor.selection} sel)` : ''}
          </span>
        )}
        <button
          type="button"
          onClick={() => setSetting('indentWithTabs', !indentWithTabs)}
          className="hover:text-zinc-200"
          title="Toggle tab/space indentation"
        >
          {indentWithTabs ? 'Tabs' : 'Spaces'}: {tabSize}
        </button>
        <button
          type="button"
          onClick={() => setSetting('wordWrap', !wordWrap)}
          className="hover:text-zinc-200"
          title="Toggle word wrap"
        >
          {wordWrap ? 'Wrap' : 'No-wrap'}
        </button>
        {lang && <span className="text-zinc-400">{lang}</span>}
      </div>
    </footer>
  )
}
