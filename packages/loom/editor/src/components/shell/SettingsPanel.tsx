import { useEffect, useState } from 'react'
import clsx from 'clsx'
import { useSettings, type EditorSettings } from '@/store/settings'

type Row<K extends keyof EditorSettings> = {
  key: K
  label: string
  description?: string
}

function NumberInput({
  value,
  onChange,
  min,
  max,
  step = 1,
}: {
  value: number
  onChange: (v: number) => void
  min?: number
  max?: number
  step?: number
}) {
  return (
    <input
      type="number"
      value={value}
      min={min}
      max={max}
      step={step}
      onChange={(e) => {
        const n = Number(e.target.value)
        if (Number.isFinite(n)) onChange(n)
      }}
      className="w-20 px-2 py-1 text-xs bg-zinc-800 border border-white/10 rounded text-white outline-none focus:border-blue-400"
    />
  )
}

function Toggle({ value, onChange }: { value: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      type="button"
      onClick={() => onChange(!value)}
      className={clsx(
        'w-9 h-5 rounded-full transition-colors relative',
        value ? 'bg-blue-500' : 'bg-zinc-700',
      )}
      aria-pressed={value}
    >
      <span
        className={clsx(
          'absolute top-0.5 h-4 w-4 rounded-full bg-white transition-transform',
          value ? 'translate-x-4' : 'translate-x-0.5',
        )}
      />
    </button>
  )
}

const BOOL_ROWS: Row<keyof EditorSettings>[] = [
  { key: 'lineNumbers', label: 'Line numbers' },
  { key: 'wordWrap', label: 'Word wrap' },
  { key: 'showIndentGuides', label: 'Indent guides' },
  { key: 'highlightActiveLine', label: 'Highlight active line' },
  { key: 'foldGutter', label: 'Code folding gutter' },
  { key: 'bracketMatching', label: 'Bracket matching' },
  { key: 'closeBrackets', label: 'Auto-close brackets' },
  { key: 'autocompletion', label: 'Autocompletion' },
  { key: 'indentWithTabs', label: 'Indent with tabs' },
  { key: 'autoSave', label: 'Auto save', description: 'Save dirty file after a short delay' },
  { key: 'formatOnSave', label: 'Format on save', description: 'Trim trailing whitespace and ensure final newline' },
]

// Loom `.loom` IDE / language-server features.
const LSP_ROWS: Row<keyof EditorSettings>[] = [
  { key: 'hoverEnabled', label: 'Hover tooltips', description: 'Docs on directives, beats, characters & traits' },
  { key: 'lspCompletion', label: 'Smart completion', description: 'Divert targets, directives & mixins from the language server' },
  { key: 'gotoOnClick', label: '⌘/Ctrl-click to definition', description: 'Also underlines jumpable symbols on ⌘/Ctrl-hover' },
  { key: 'occurrenceHighlight', label: 'Highlight occurrences', description: 'Every use of the identifier under the cursor' },
  { key: 'projectDiagnostics', label: 'Project diagnostics', description: 'Surface cross-file errors, not just parser errors' },
  { key: 'indexWholeProject', label: 'Index whole project', description: 'Enable cross-file go-to-definition & references' },
]

export function SettingsPanel() {
  const [isOpen, setOpen] = useState(false)
  const s = useSettings()

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === ',') {
        e.preventDefault()
        setOpen((v) => !v)
      } else if (e.key === 'Escape' && isOpen) {
        setOpen(false)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [isOpen])

  if (!isOpen) return null

  return (
    <div
      className="fixed inset-0 z-50 bg-black/50 flex items-start justify-center pt-16"
      onClick={() => setOpen(false)}
    >
      <div
        className="w-[560px] max-w-[92vw] max-h-[80vh] bg-zinc-900 border border-white/10 rounded-lg shadow-2xl overflow-hidden flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 py-3 border-b border-white/10 flex items-center justify-between">
          <div className="text-sm font-semibold">Settings</div>
          <button
            type="button"
            onClick={() => s.reset()}
            className="text-xs text-zinc-400 hover:text-white"
          >
            Reset to defaults
          </button>
        </div>
        <div className="flex-1 overflow-auto px-4 py-3 space-y-4 text-sm">
          <Section title="Appearance">
            <Field label="Theme">
              <select
                value={s.theme}
                onChange={(e) => s.set('theme', e.target.value as 'dark' | 'light')}
                className="px-2 py-1 text-xs bg-zinc-800 border border-white/10 rounded text-white outline-none"
              >
                <option value="dark">Dark (One Dark)</option>
                <option value="light">Light</option>
              </select>
            </Field>
            <Field label="Font size">
              <NumberInput value={s.fontSize} min={9} max={32} onChange={(v) => s.set('fontSize', v)} />
            </Field>
            <Field label="Font family">
              <input
                type="text"
                value={s.fontFamily}
                onChange={(e) => s.set('fontFamily', e.target.value)}
                className="w-full px-2 py-1 text-xs bg-zinc-800 border border-white/10 rounded text-white outline-none focus:border-blue-400"
              />
            </Field>
          </Section>

          <Section title="Indentation">
            <Field label="Tab size">
              <NumberInput value={s.tabSize} min={1} max={8} onChange={(v) => s.set('tabSize', v)} />
            </Field>
          </Section>

          <Section title="Editor">
            {BOOL_ROWS.map(({ key, label, description }) => (
              <Field key={key} label={label} description={description}>
                <Toggle
                  value={Boolean(s[key])}
                  onChange={(v) => s.set(key, v as never)}
                />
              </Field>
            ))}
            {s.autoSave && (
              <Field label="Auto save delay (ms)">
                <NumberInput
                  value={s.autoSaveDelayMs}
                  min={200}
                  max={10000}
                  step={100}
                  onChange={(v) => s.set('autoSaveDelayMs', v)}
                />
              </Field>
            )}
          </Section>

          <Section title="Loom IDE">
            {LSP_ROWS.map(({ key, label, description }) => (
              <Field key={key} label={label} description={description}>
                <Toggle value={Boolean(s[key])} onChange={(v) => s.set(key, v as never)} />
              </Field>
            ))}
            {s.hoverEnabled && (
              <Field label="Hover delay (ms)" description="⌘/Ctrl-hover shows instantly">
                <NumberInput
                  value={s.hoverDelayMs}
                  min={0}
                  max={2000}
                  step={50}
                  onChange={(v) => s.set('hoverDelayMs', v)}
                />
              </Field>
            )}
          </Section>
        </div>
        <div className="px-4 py-2 border-t border-white/10 text-[11px] text-zinc-500">
          Settings persist in localStorage.
        </div>
      </div>
    </div>
  )
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="text-[11px] uppercase tracking-wider text-zinc-500 mb-2">{title}</div>
      <div className="space-y-2">{children}</div>
    </div>
  )
}

function Field({
  label,
  description,
  children,
}: {
  label: string
  description?: string
  children: React.ReactNode
}) {
  return (
    <div className="flex items-center justify-between gap-3">
      <div>
        <div className="text-zinc-200">{label}</div>
        {description && <div className="text-[11px] text-zinc-500">{description}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  )
}
