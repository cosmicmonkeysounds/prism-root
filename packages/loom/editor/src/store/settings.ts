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
