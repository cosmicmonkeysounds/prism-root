import { test, expect, type Page } from '@playwright/test'

const tab = (page: Page, title: string) =>
  page.locator('.dv-default-tab').filter({ hasText: title })

const activity = (page: Page, id: 'files' | 'search' | 'editor' | 'canvas') =>
  page.getByTestId(`activity-${id}`)

test.describe('Loom dock shell', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await expect(page.getByRole('banner').getByText('Loom')).toBeVisible()
  })

  test('shows the three default panels', async ({ page }) => {
    await expect(tab(page, 'Files')).toBeVisible()
    await expect(tab(page, 'Editor')).toBeVisible()
    await expect(tab(page, 'Canvas')).toBeVisible()
  })

  test('activity bar is always visible with all three toggles', async ({ page }) => {
    await expect(page.getByTestId('activity-bar')).toBeVisible()
    for (const id of ['files', 'editor', 'canvas'] as const) {
      await expect(activity(page, id)).toBeVisible()
      await expect(activity(page, id)).toHaveAttribute('aria-pressed', 'true')
    }
  })

  test('toggling a panel from the activity bar closes and reopens it', async ({ page }) => {
    await activity(page, 'canvas').click()
    await expect(tab(page, 'Canvas')).toHaveCount(0)
    await expect(activity(page, 'canvas')).toHaveAttribute('aria-pressed', 'false')

    await activity(page, 'canvas').click()
    await expect(tab(page, 'Canvas')).toBeVisible()
    await expect(activity(page, 'canvas')).toHaveAttribute('aria-pressed', 'true')
  })

  test('reopened files panel returns to the leftmost slot', async ({ page }) => {
    // Close Files via its tab action so it disappears
    const filesTab = tab(page, 'Files')
    await filesTab.hover()
    await filesTab.locator('.dv-default-tab-action').click()
    await expect(tab(page, 'Files')).toHaveCount(0)

    // Reopen from the activity bar
    await activity(page, 'files').click()
    await expect(tab(page, 'Files')).toBeVisible()

    // Files group should sit to the left of the Editor group
    const filesBox = await tab(page, 'Files').boundingBox()
    const editorBox = await tab(page, 'Editor').boundingBox()
    expect(filesBox && editorBox).toBeTruthy()
    expect(filesBox!.x).toBeLessThan(editorBox!.x)
  })

  test('reopened canvas panel returns below the editor', async ({ page }) => {
    const canvasTab = tab(page, 'Canvas')
    await canvasTab.hover()
    await canvasTab.locator('.dv-default-tab-action').click()
    await expect(tab(page, 'Canvas')).toHaveCount(0)

    await activity(page, 'canvas').click()
    await expect(tab(page, 'Canvas')).toBeVisible()

    const editorBox = await tab(page, 'Editor').boundingBox()
    const canvasBox = await tab(page, 'Canvas').boundingBox()
    expect(editorBox && canvasBox).toBeTruthy()
    expect(canvasBox!.y).toBeGreaterThan(editorBox!.y)
  })

  test('closes every panel and restores them all in order', async ({ page }) => {
    for (const title of ['Files', 'Editor', 'Canvas']) {
      const t = tab(page, title)
      await t.hover()
      await t.locator('.dv-default-tab-action').click()
    }
    for (const id of ['files', 'editor', 'canvas'] as const) {
      await expect(activity(page, id)).toHaveAttribute('aria-pressed', 'false')
    }

    for (const id of ['files', 'editor', 'canvas'] as const) {
      await activity(page, id).click()
    }
    await expect(tab(page, 'Files')).toBeVisible()
    await expect(tab(page, 'Editor')).toBeVisible()
    await expect(tab(page, 'Canvas')).toBeVisible()
  })

  test('keyboard shortcut toggles the files panel', async ({ page }) => {
    const isMac = process.platform === 'darwin'
    const mod = isMac ? 'Meta' : 'Control'
    await page.keyboard.press(`${mod}+b`)
    await expect(tab(page, 'Files')).toHaveCount(0)
    await page.keyboard.press(`${mod}+b`)
    await expect(tab(page, 'Files')).toBeVisible()
  })

  test('command palette opens with Cmd/Ctrl+P', async ({ page }) => {
    const isMac = process.platform === 'darwin'
    await page.keyboard.press(isMac ? 'Meta+P' : 'Control+P')
    await expect(page.getByPlaceholder('Go to file…')).toBeVisible()
    await page.keyboard.press('Escape')
    await expect(page.getByPlaceholder('Go to file…')).toBeHidden()
  })
})
