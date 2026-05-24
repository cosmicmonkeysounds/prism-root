import { test, expect, type Page } from '@playwright/test'

const mod = process.platform === 'darwin' ? 'Meta' : 'Control'

test.describe('Loom new features', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await expect(page.getByRole('banner').getByText('Loom')).toBeVisible()
  })

  test('command palette has command mode via Cmd+Shift+P', async ({ page }) => {
    await page.keyboard.press(`${mod}+Shift+P`)
    await expect(page.getByPlaceholder('Run command…')).toBeVisible()
    // Typing the prefix should still work from file mode
    await page.keyboard.press('Escape')
    await page.keyboard.press(`${mod}+P`)
    const input = page.getByPlaceholder(/Go to file/)
    await expect(input).toBeVisible()
    await input.fill('>save')
    // A command like "File: Save" should be visible
    await expect(page.getByText('File: Save', { exact: true })).toBeVisible()
  })

  test('settings panel opens with Cmd+, and toggles theme', async ({ page }) => {
    await page.keyboard.press(`${mod}+Comma`)
    await expect(page.getByText('Settings', { exact: true })).toBeVisible()
    // initial theme attribute
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
    // switch to light via the select
    await page.locator('select').first().selectOption('light')
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light')
    // restore
    await page.locator('select').first().selectOption('dark')
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
    await page.keyboard.press('Escape')
    await expect(page.getByText('Settings', { exact: true })).toBeHidden()
  })

  test('search panel toggles with Cmd+Shift+F', async ({ page }) => {
    // It is hidden by default
    await expect(page.getByPlaceholder('Search workspace…')).toBeHidden()
    await page.keyboard.press(`${mod}+Shift+F`)
    await expect(page.getByPlaceholder('Search workspace…')).toBeVisible()
    await page.keyboard.press(`${mod}+Shift+F`)
    await expect(page.getByPlaceholder('Search workspace…')).toBeHidden()
  })

  test('command palette → open settings runs the command', async ({ page }) => {
    await page.keyboard.press(`${mod}+Shift+P`)
    const input = page.getByPlaceholder('Run command…')
    await input.fill('open settings')
    await page.keyboard.press('Enter')
    await expect(page.getByText('Settings', { exact: true })).toBeVisible()
  })
})

test.describe('Loom command palette → existing behavior still works', () => {
  test('file picker placeholder still matches', async ({ page }: { page: Page }) => {
    await page.goto('/')
    await expect(page.getByRole('banner').getByText('Loom')).toBeVisible()
    await page.keyboard.press(`${mod}+P`)
    await expect(page.getByPlaceholder(/Go to file/)).toBeVisible()
  })
})
