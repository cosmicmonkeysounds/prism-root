// The Studio shell: the four-mode Mode Bar (Writing / Editing / Sim /
// Run), ⌘1..⌘4 switching, and per-mode region composition.

import { test, expect } from '@playwright/test'
import { openProject, switchMode } from './helpers'

const mod = process.platform === 'darwin' ? 'Meta' : 'Control'

test.describe('studio shell', () => {
  test.beforeEach(async ({ page }) => {
    await openProject(page)
  })

  test('mode bar shows all four modes with keybinding hints', async ({ page }) => {
    for (const id of ['writing', 'editing', 'sim', 'operate'] as const) {
      await expect(page.getByTestId(`mode-${id}`)).toBeVisible()
    }
    await expect(page.getByTestId('mode-sim')).toContainText('Sim')
    await expect(page.getByTestId('mode-operate')).toContainText('Run')
  })

  test('modes switch by click and by ⌘1..⌘4', async ({ page }) => {
    await switchMode(page, 'editing')
    await expect(page.getByTestId('mode-editing')).toHaveAttribute('aria-pressed', 'true')
    await expect(page.getByTestId('graph-crumb-project')).toBeVisible()

    await page.keyboard.press(`${mod}+3`)
    await expect(page.getByTestId('mode-sim')).toHaveAttribute('aria-pressed', 'true')
    await expect(page.getByTestId('sim-start')).toBeVisible()

    await page.keyboard.press(`${mod}+4`)
    await expect(page.getByTestId('mode-operate')).toHaveAttribute('aria-pressed', 'true')

    await page.keyboard.press(`${mod}+1`)
    await expect(page.getByTestId('mode-writing')).toHaveAttribute('aria-pressed', 'true')
  })

  test('writing mode shows the file tree and editor stack', async ({ page }) => {
    await switchMode(page, 'writing')
    await expect(page.getByText('main.loom').first()).toBeVisible()
  })
})
