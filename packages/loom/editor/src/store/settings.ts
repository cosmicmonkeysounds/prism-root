import { create } from 'zustand'
import { persist, createJSONStorage } from 'zustand/middleware'

export type ThemeChoice = 'dark' | 'light'

export type EditorSettings = {
  theme: ThemeChoice
  fontSize: number
  fontFamily: string
  tabSize: number
  indentWithTabs: boolean
  wordWrap: boolean
  lineNumbers: boolean
  showIndentGuides: boolean
  formatOnSave: boolean
  autoSave: boolean
  autoSaveDelayMs: number
  highlightActiveLine: boolean
  bracketMatching: boolean
  closeBrackets: boolean
  autocompletion: boolean
  foldGutter: boolean
  // ── IDE / LSP (Loom `.loom` files) ──────────────────────────────────
  /** Show markdown hover tooltips over directives / beats / characters. */
  hoverEnabled: boolean
  /** Delay before a hover tooltip appears, ms (⌘/Ctrl-hover is instant). */
  hoverDelayMs: number
  /** Use the Loom language server for completion (vs. generic word list). */
  lspCompletion: boolean
  /** ⌘/Ctrl-Click a symbol to jump to its definition. */
  gotoOnClick: boolean
  /** Highlight every occurrence of the identifier under the cursor. */
  occurrenceHighlight: boolean
  /** Surface cross-file project diagnostics in the gutter, not just parser errors. */
  projectDiagnostics: boolean
  /** Index every `.loom` in the project (not just open tabs) for cross-file nav. */
  indexWholeProject: boolean
}

type SettingsState = EditorSettings & {
  set: <K extends keyof EditorSettings>(key: K, value: EditorSettings[K]) => void
  reset: () => void
}

const DEFAULTS: EditorSettings = {
  theme: 'dark',
  fontSize: 13,
  fontFamily:
    'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace',
  tabSize: 2,
  indentWithTabs: false,
  wordWrap: false,
  lineNumbers: true,
  showIndentGuides: true,
  formatOnSave: false,
  autoSave: false,
  autoSaveDelayMs: 1000,
  highlightActiveLine: true,
  bracketMatching: true,
  closeBrackets: true,
  autocompletion: true,
  foldGutter: true,
  hoverEnabled: true,
  hoverDelayMs: 500,
  lspCompletion: true,
  gotoOnClick: true,
  occurrenceHighlight: true,
  projectDiagnostics: true,
  indexWholeProject: true,
}

export const useSettings = create<SettingsState>()(
  persist(
    (set) => ({
      ...DEFAULTS,
      set: (key, value) => set({ [key]: value } as Partial<SettingsState>),
      reset: () => set({ ...DEFAULTS }),
    }),
    {
      name: 'loom-editor-settings',
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => {
        const { set: _s, reset: _r, ...rest } = s
        return rest
      },
    },
  ),
)
