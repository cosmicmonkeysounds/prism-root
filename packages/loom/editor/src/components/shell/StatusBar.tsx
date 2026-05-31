import { useWorkspace } from '@/store/workspace'
import { useSettings } from '@/store/settings'
import { languageForPath, languageLabel } from '@/lib/language'
import { usePresets } from '@/store/presets'
import { getActiveDockApi } from '@/components/dock/util'

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
        <PresetSwitcher />
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

function PresetSwitcher() {
  const presets = usePresets((s) => s.all())
  const active = usePresets((s) => s.active)
  const apply = usePresets((s) => s.apply)
  const save = usePresets((s) => s.saveCurrent)
  const del = usePresets((s) => s.delete)
  const current = presets.find((p) => p.id === active)
  return (
    <div className="flex items-center gap-1">
      <select
        aria-label="Workspace preset"
        value={active ?? ''}
        onChange={(e) => {
          const api = getActiveDockApi()
          if (!api) return
          if (e.target.value) apply(api, e.target.value)
        }}
        className="bg-zinc-950 text-zinc-300 border border-white/10 rounded px-1 py-px hover:text-zinc-100"
        title="Switch workspace preset (⌘⌥1..5 for builtins)"
      >
        <option value="">Workspace…</option>
        {presets.map((p) => (
          <option key={p.id} value={p.id}>
            {p.builtin ? '◇ ' : ''}
            {p.name}
          </option>
        ))}
      </select>
      <button
        type="button"
        onClick={() => {
          const api = getActiveDockApi()
          if (!api) return
          const name = window.prompt('Preset name', current?.name ?? 'My workspace')
          if (name) save(api, name)
        }}
        className="hover:text-zinc-200"
        title="Save current layout as a new preset"
      >
        +
      </button>
      {current && !current.builtin && (
        <button
          type="button"
          onClick={() => {
            if (window.confirm(`Delete preset "${current.name}"?`)) del(current.id)
          }}
          className="hover:text-rose-400"
          title="Delete the active preset"
        >
          ×
        </button>
      )}
    </div>
  )
}
