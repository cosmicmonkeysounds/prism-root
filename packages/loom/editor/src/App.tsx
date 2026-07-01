import { useEffect } from 'react'
import { StudioShell } from '@/components/studio/StudioShell'
import { TopBar } from '@/components/studio/TopBar'
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
      <TopBar />

      <main className="flex-1 min-h-0">
        <StudioShell />
      </main>

      <StatusBar />
      <CommandPalette />
      <SettingsPanel />
      <DetailOverlay />
      <ContextMenuHost />
    </div>
  )
}
