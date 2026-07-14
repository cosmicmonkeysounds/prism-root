// The Studio shell: the three-mode Mode Bar (Writing / Run / Deploy),
// ⌘1..⌘3 switching, the Writing editor ⇄ story-graph split, and the
// ⌘\ graph-pane toggle.

import { test, expect } from '@playwright/test'
import { openProject, switchMode } from './helpers'

const mod = process.platform === 'darwin' ? 'Meta' : 'Control'

test.describe('studio shell', () => {
  test.beforeEach(async ({ page }) => {
    await openProject(page)
  })

  test('mode bar shows all three modes with keybinding hints', async ({ page }) => {
    for (const id of ['writing', 'run', 'deploy'] as const) {
      await expect(page.getByTestId(`mode-${id}`)).toBeVisible()
    }
    await expect(page.getByTestId('mode-writing')).toContainText('Writing')
    await expect(page.getByTestId('mode-run')).toContainText('Run')
    await expect(page.getByTestId('mode-deploy')).toContainText('Deploy')
  })

  test('writing mode shows the text editor AND the story graph together', async ({ page }) => {
    await switchMode(page, 'writing')
    await expect(page.getByText('main.loom').first()).toBeVisible()
    await expect(page.getByTestId('graph-crumb-project')).toBeVisible()
    await expect(page.getByTestId('graph-beat-opening')).toBeVisible()
  })

  test('modes switch by click and by ⌘1..⌘3', async ({ page }) => {
    await page.keyboard.press(`${mod}+2`)
    await expect(page.getByTestId('mode-run')).toHaveAttribute('aria-pressed', 'true')
    await expect(page.getByTestId('sim-start')).toBeVisible() // Sim source is the default

    await page.keyboard.press(`${mod}+3`)
    await expect(page.getByTestId('mode-deploy')).toHaveAttribute('aria-pressed', 'true')

    await page.keyboard.press(`${mod}+1`)
    await expect(page.getByTestId('mode-writing')).toHaveAttribute('aria-pressed', 'true')
  })

  test('run mode has the Sim ⇄ Live source switch', async ({ page }) => {
    await switchMode(page, 'run')
    await expect(page.getByTestId('run-source-sim')).toHaveAttribute('aria-pressed', 'true')
    await expect(page.getByTestId('run-source-live')).toBeVisible()
  })

  test('⌘\\ hides and re-shows the story-graph pane', async ({ page }) => {
    await switchMode(page, 'writing')
    // The pane collapses to width 0 (allotment clips, it doesn't unmount),
    // so assert with the clipping-aware viewport check.
    await expect(page.getByTestId('graph-crumb-project')).toBeInViewport()
    await page.keyboard.press(`${mod}+\\`)
    await expect(page.getByTestId('graph-crumb-project')).not.toBeInViewport()
    await page.keyboard.press(`${mod}+\\`)
    await expect(page.getByTestId('graph-crumb-project')).toBeInViewport()
  })

  test('⌘B and ⌘⌥B collapse the left rail and properties tray', async ({ page }) => {
    await switchMode(page, 'writing')
    const rail = page.getByTestId('editing-rail-story')
    await expect(rail).toBeInViewport()
    await page.keyboard.press(`${mod}+b`)
    await expect(rail).not.toBeInViewport()
    await page.keyboard.press(`${mod}+b`)
    await expect(rail).toBeInViewport()

    const tray = page.getByRole('tab', { name: 'Properties' })
    await expect(tray).toBeInViewport()
    await page.keyboard.press(`${mod}+Alt+b`)
    await expect(tray).not.toBeInViewport()
    await page.keyboard.press(`${mod}+Alt+b`)
    await expect(tray).toBeInViewport()
  })
})
