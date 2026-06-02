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
import { useSession } from '@/store/session'

export default function App() {
  const restoreRoot = useWorkspace((s) => s.restoreRoot)
  const root = useWorkspace((s) => s.root)
  const theme = useSettings((s) => s.theme)
  const activateLocal = useSession((s) => s.activateLocal)
  const activeKind = useSession((s) => s.active?.kind ?? null)
  const activeId = useSession((s) => s.active?.meta.id ?? null)
  useEffect(() => {
    void restoreRoot()
  }, [restoreRoot])
  useEffect(() => {
    document.documentElement.dataset.theme = theme
  }, [theme])
  // Ensure a local (no-relay) workspace is always active so play works
  // out of the box. Skips when a cloud workspace is open; re-targets
  // when the open folder changes; falls back to the bundled example.
  useEffect(() => {
    if (activeKind === 'cloud') return
    void activateLocal()
  }, [activeKind, activeId, root, activateLocal])

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
