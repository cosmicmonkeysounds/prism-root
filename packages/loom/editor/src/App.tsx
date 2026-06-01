import { useEffect } from 'react'
import { DockShell } from '@/components/dock/DockShell'
import { StatusBar } from '@/components/shell/StatusBar'
import { CommandPalette } from '@/components/shell/CommandPalette'
import { SettingsPanel } from '@/components/shell/SettingsPanel'
import { DetailOverlay } from '@/components/detail/DetailOverlay'
import { ContextMenuHost } from '@/components/shell/ContextMenu'
import { useWorkspace } from '@/store/workspace'
import { useSettings } from '@/store/settings'

export default function App() {
  const restoreRoot = useWorkspace((s) => s.restoreRoot)
  const theme = useSettings((s) => s.theme)
  useEffect(() => {
    void restoreRoot()
  }, [restoreRoot])
  useEffect(() => {
    document.documentElement.dataset.theme = theme
  }, [theme])

  return (
    <div className="h-full w-full flex flex-col">
      <header className="h-9 px-3 flex items-center border-b border-white/10 bg-zinc-950 text-sm">
        <span className="font-semibold tracking-wide">Loom</span>
        <span className="ml-2 text-zinc-500 text-xs">IDE + Canvas</span>
      </header>

      <main className="flex-1 min-h-0">
        <DockShell />
      </main>

      <StatusBar />
      <CommandPalette />
      <SettingsPanel />
      <DetailOverlay />
      <ContextMenuHost />
    </div>
  )
}
