import { useEffect } from 'react'
import { StudioShell } from '@/components/studio/StudioShell'
import { TopBar } from '@/components/studio/TopBar'
import { StatusBar } from '@/components/shell/StatusBar'
import { CommandPalette } from '@/components/shell/CommandPalette'
import { SettingsPanel } from '@/components/shell/SettingsPanel'
import { DetailOverlay } from '@/components/detail/DetailOverlay'
import { ContextMenuHost } from '@/components/shell/ContextMenu'
import { AuthGate } from '@/components/auth/AuthGate'
import { ProjectsLaunchpad } from '@/components/projects/ProjectsLaunchpad'
import { useWorkspace } from '@/store/workspace'
import { useSettings } from '@/store/settings'
import { useAuth } from '@/store/auth'

export default function App() {
  const restoreRoot = useWorkspace((s) => s.restoreRoot)
  const root = useWorkspace((s) => s.root)
  const theme = useSettings((s) => s.theme)
  const authStatus = useAuth((s) => s.status)
  const refreshAuth = useAuth((s) => s.refresh)

  useEffect(() => {
    void refreshAuth()
    void restoreRoot()
  }, [refreshAuth, restoreRoot])
  useEffect(() => {
    document.documentElement.dataset.theme = theme
  }, [theme])

  // A workspace (local folder or server project) is open → the Studio shell.
  if (root) {
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

  // Otherwise gate on the author account: signed in → launchpad, else login.
  if (authStatus === 'loading') {
    return <div className="h-full w-full grid place-items-center bg-zinc-950 text-zinc-500 text-sm">Loading…</div>
  }
  if (authStatus === 'signed-in') return <ProjectsLaunchpad />
  return <AuthGate />
}
