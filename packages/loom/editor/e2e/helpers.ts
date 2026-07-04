// Shared helpers for the editor e2e suite. The suite runs against the
// Vite dev server (see playwright.config.ts webServer) with no backend:
// a workspace is injected straight into the zustand store through
// Vite's module graph (`/src/store/workspace.ts` resolves to the same
// module instance the app uses), which mirrors a server project being
// opened without needing auth or a Postgres control plane.

import { expect, type Page } from '@playwright/test'

/** A small but representative project: entry beat, dialogue, choices,
 *  cross-beat diverts, a character with a scan hook + a named event. */
export const MAIN_LOOM = `entry: opening

FACTION Mods
  ethos: order

LOCATION Party
  label: The Party

ROLE Guest
  score: 0 to 100 = 0

CHARACTER Greeter
  faction: Mods
  on lockdown
    <broadcast: lockdown_siren to faction(Mods)>
  on scan guest
    <set: guest.score += 5>

== opening
  The lights dim.
  GREETER
    Welcome, traveler.
  * Take the stairs
    -> stairs
  * Take the lift
    -> lift

== stairs
  You climb into the dark.
  -> summit

== lift
  You ride. Muzak plays.
  -> summit

== summit
  The city glitters below.
  -> END
`

/** Open the app and inject `files` as an in-memory server project. */
export async function openProject(
  page: Page,
  files: Array<{ path: string; content: string }> = [{ path: 'main.loom', content: MAIN_LOOM }],
): Promise<void> {
  await page.goto('/')
  await page.waitForLoadState('networkidle')
  await page.evaluate(async (fs) => {
    const ws = await import('/src/store/workspace.ts')
    await ws.useWorkspace.getState().openServerProject({ id: 'e2e', name: 'e2e-project' }, fs)
  }, files)
  // The studio shell mounts once the workspace root lands; indexing is
  // debounced, so wait for the mode bar and give the LSP a beat.
  await expect(page.getByTestId('mode-bar')).toBeVisible()
  await page.waitForTimeout(600)
}

/** Switch modes via the Mode Bar. */
export async function switchMode(
  page: Page,
  mode: 'writing' | 'editing' | 'sim' | 'operate',
): Promise<void> {
  await page.getByTestId(`mode-${mode}`).click()
}
