// Writing-mode quality-of-life: the two panes track each other (graph
// drill-in ⇄ text cursor), the text editor has its own contextual menu
// with LSP actions, canvas edits undo from anywhere via the story edit
// journal, and cockpit messages have real context menus.

import { test, expect } from '@playwright/test'
import { openProject, switchMode } from './helpers'

const mod = process.platform === 'darwin' ? 'Meta' : 'Control'

// MAIN_LOOM landmarks (1-based lines): `== stairs` 28, its prose 29,
// `-> summit` 30, `== summit` 36.

test.describe('writing QoL', () => {
  test.beforeEach(async ({ page }) => {
    await openProject(page)
    await switchMode(page, 'writing')
    await expect(page.getByTestId('graph-beat-opening')).toBeVisible()
    await page.waitForTimeout(1200)
  })

  test('drilling into a beat lines the text editor up on it, without stealing focus', async ({ page }) => {
    await page.getByTestId('graph-beat-summit').dblclick()
    await expect(page.getByTestId('graph-crumb-beat')).toHaveText('summit')
    // The text pane revealed `== summit` …
    await expect
      .poll(async () =>
        page.evaluate(async () => {
          const ws = await import('/src/store/workspace.ts')
          return ws.useWorkspace.getState().cursor?.line ?? null
        }),
      )
      .toBe(36)
    // … but the keyboard stayed on the canvas (Esc must still back out).
    const focusInEditor = await page.evaluate(
      () => document.activeElement?.closest('.cm-editor') !== null && document.activeElement !== document.body,
    )
    expect(focusInEditor).toBe(false)
    await page.keyboard.press('Escape')
    await expect(page.getByTestId('graph-crumb-beat')).toHaveCount(0)
  })

  test('moving the text cursor selects the enclosing beat on the canvas', async ({ page }) => {
    await page.locator('.cm-line', { hasText: 'You climb into the dark.' }).click()
    await expect(page.locator('.react-flow__node.selected')).toHaveCount(1)
    expect(
      await page.locator('.react-flow__node.selected').getAttribute('data-id'),
    ).toBe('stairs')
  })

  test('the editor context menu offers LSP actions and works', async ({ page }) => {
    // Right-click the `summit` divert target inside `== stairs`.
    await page.locator('.cm-content').getByText('summit', { exact: true }).first().click({ button: 'right' })
    await expect(page.getByRole('menuitem', { name: 'Go to definition' })).toBeEnabled()
    await expect(page.getByRole('menuitem', { name: /Rename/ })).toBeEnabled()
    await expect(page.getByRole('menuitem', { name: 'Paste' })).toBeVisible()

    // Reveal in story graph → the enclosing beat selects + centers.
    await page.getByRole('menuitem', { name: 'Reveal in story graph' }).click()
    await expect(page.locator('.react-flow__node.selected')).toHaveCount(1)
    expect(
      await page.locator('.react-flow__node.selected').getAttribute('data-id'),
    ).toBe('stairs')

    // Go to definition → the cursor lands on `== summit`.
    await page.locator('.cm-content').getByText('summit', { exact: true }).first().click({ button: 'right' })
    await page.getByRole('menuitem', { name: 'Go to definition' }).click()
    await expect
      .poll(async () =>
        page.evaluate(async () => {
          const ws = await import('/src/store/workspace.ts')
          return ws.useWorkspace.getState().cursor?.line ?? null
        }),
      )
      .toBe(36)
  })

  test('⌘Z outside the text editor undoes a canvas edit (and ⌘⇧Z redoes)', async ({ page }) => {
    page.on('dialog', (d) => void d.accept('Jump out the window'))
    const source = () =>
      page.evaluate(async () => {
        const ws = await import('/src/store/workspace.ts')
        return ws.useWorkspace.getState().openFiles['main.loom']?.contents ?? ''
      })

    await page.getByTestId('graph-beat-opening').click({ button: 'right' })
    await page.getByTestId('graph-menu-add-choice').click()
    await expect.poll(async () => (await source()).includes('Jump out the window')).toBe(true)

    // Keyboard on the canvas — ⌘Z routes to the story edit journal.
    await page.getByTestId('graph-beat-opening').click()
    await page.keyboard.press(`${mod}+z`)
    await expect.poll(async () => (await source()).includes('Jump out the window')).toBe(false)

    await page.keyboard.press(`${mod}+Shift+z`)
    await expect.poll(async () => (await source()).includes('Jump out the window')).toBe(true)
  })
})

test.describe('cockpit chat QoL', () => {
  test.beforeEach(async ({ page }) => {
    await openProject(page)
    await switchMode(page, 'run')
    await page.getByTestId('sim-start').click()
    await page.waitForTimeout(500)
    await page.getByRole('tab', { name: 'Chat' }).click()
  })

  test('message context menu: copy / reply / hide are offered, reply threads', async ({ page }) => {
    const first = page.locator('li.group').first()
    await first.click({ button: 'right' })
    await expect(page.getByRole('menuitem', { name: 'Copy text' })).toBeVisible()
    await expect(page.getByTestId('chat-menu-hide')).toBeVisible()

    await page.getByTestId('chat-menu-reply').click()
    await expect(page.getByTestId('chat-reply-chip')).toBeVisible()

    await page.locator('input[placeholder^="Message"]').fill('Roger that')
    await page.keyboard.press('Enter')
    await expect(page.getByText('Roger that')).toBeVisible()
    // Sending clears the reply state.
    await expect(page.getByTestId('chat-reply-chip')).toHaveCount(0)
  })

  test('hide via the context menu dims the message', async ({ page }) => {
    const first = page.locator('li.group').first()
    await first.click({ button: 'right' })
    await page.getByTestId('chat-menu-hide').click()
    await expect(page.locator('li.group.opacity-40').first()).toBeVisible()
  })
})
