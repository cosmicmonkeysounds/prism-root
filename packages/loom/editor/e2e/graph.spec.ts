// The story-graph canvas (Writing mode's right pane): node cards,
// word-block expand/collapse (cards stretch to fit), genuine node
// dragging (moves stick across re-renders), and the beat drill-in.

import { test, expect } from '@playwright/test'
import { openProject, switchMode } from './helpers'

test.describe('story-graph canvas', () => {
  test.beforeEach(async ({ page }) => {
    await openProject(page)
    await switchMode(page, 'writing')
    // First ELK layout + the measured second pass.
    await expect(page.getByTestId('graph-beat-opening')).toBeVisible()
    await page.waitForTimeout(1200)
  })

  test('project map renders every beat as a card', async ({ page }) => {
    for (const key of ['opening', 'stairs', 'lift', 'summit']) {
      await expect(page.getByTestId(`graph-beat-${key}`)).toBeVisible()
    }
  })

  test('word blocks expand and collapse on a beat card', async ({ page }) => {
    const beat = page.getByTestId('graph-beat-opening')
    // Collapsed: preview only — the second choice is beyond the 3-line cap.
    await expect(beat).not.toContainText('Take the lift')

    await page.getByTestId('graph-beat-expand-opening').click()
    await page.waitForTimeout(900) // block build + relayout
    await expect(beat).toContainText('Welcome, traveler.')
    await expect(beat).toContainText('Take the stairs')
    await expect(beat).toContainText('Take the lift')
    await expect(beat).toContainText('-> stairs')

    await page.getByTestId('graph-beat-expand-opening').click()
    await page.waitForTimeout(600)
    await expect(beat).not.toContainText('Take the lift')
  })

  test('the toolbar blocks toggle expands every card at once', async ({ page }) => {
    await page.getByTestId('graph-overlay-blocks').click()
    await page.waitForTimeout(1200)
    await expect(page.getByTestId('graph-beat-summit')).toContainText('The city glitters below.')
    await expect(page.getByTestId('graph-beat-opening')).toContainText('Take the lift')
    await page.getByTestId('graph-overlay-blocks').click()
    await page.waitForTimeout(600)
    // Collapse restores the 3-line preview — assert on content BEYOND the
    // preview cap (summit's whole 2-line body fits inside its preview).
    await expect(page.getByTestId('graph-beat-opening')).not.toContainText('Take the lift')
  })

  test('dragging a node moves it and the position sticks', async ({ page }) => {
    const beat = page.getByTestId('graph-beat-summit')
    const before = (await beat.boundingBox())!
    await page.mouse.move(before.x + before.width / 2, before.y + 8)
    await page.mouse.down()
    // Modest delta, down-LEFT — a long rightward drag reaches the
    // (narrower, split-pane) canvas edge and auto-pans the viewport,
    // and summit is the rightmost node, next to the minimap corner.
    await page.mouse.move(before.x + before.width / 2 - 100, before.y + 90, { steps: 10 })
    await page.mouse.up()
    await page.waitForTimeout(400)

    const after = (await beat.boundingBox())!
    expect(Math.abs(after.x - before.x)).toBeGreaterThan(50)
    expect(Math.abs(after.y - before.y)).toBeGreaterThan(45)

    // A selection click re-decorates every node — the move must survive.
    // (Click the dragged node itself: it is certainly on-screen.)
    await beat.click({ position: { x: 10, y: 8 } })
    await page.waitForTimeout(300)
    const settled = (await beat.boundingBox())!
    expect(Math.abs(settled.x - after.x)).toBeLessThan(5)
    expect(Math.abs(settled.y - after.y)).toBeLessThan(5)
  })

  test('double-click drills into a beat and Esc backs out', async ({ page }) => {
    await page.getByTestId('graph-beat-opening').dblclick()
    await expect(page.getByTestId('graph-crumb-beat')).toHaveText('opening')
    await page.keyboard.press('Escape')
    await expect(page.getByTestId('graph-crumb-beat')).toHaveCount(0)
  })

  test('a file container collapses to a compact node and expands back', async ({ page }) => {
    await page.getByTestId('graph-file-collapse-main.loom').click()
    await page.waitForTimeout(900) // relayout
    await expect(page.getByTestId('graph-beat-opening')).toHaveCount(0)
    await expect(page.getByTestId('graph-file-main.loom')).toContainText('4 beats')

    // Double-click re-expands in place (an expanded container's
    // double-click keeps opening the file in Writing mode instead).
    await page.getByTestId('graph-file-main.loom').dblclick()
    await page.waitForTimeout(900)
    await expect(page.getByTestId('graph-beat-opening')).toBeVisible()
  })

  test('typing in the toolbar input dims non-matching nodes live', async ({ page }) => {
    await page.getByTestId('graph-search').fill('stairs')
    await expect(page.locator('.react-flow__node[data-id="summit"]')).toHaveCSS('opacity', '0.15')
    await expect(page.locator('.react-flow__node[data-id="stairs"]')).toHaveCSS('opacity', '1')
    await page.getByTestId('graph-search').fill('')
    await expect(page.locator('.react-flow__node[data-id="summit"]')).toHaveCSS('opacity', '1')
  })

  test('arrow keys walk selection and Enter drills in', async ({ page }) => {
    await page.getByTestId('graph-beat-opening').click()
    await page.keyboard.press('ArrowRight')
    const selected = page.locator('.react-flow__node.selected')
    await expect(selected).toHaveCount(1)
    expect(await selected.getAttribute('data-id')).not.toBe('opening')

    await page.keyboard.press('Enter')
    await expect(page.getByTestId('graph-crumb-beat')).toBeVisible()
  })

  test('double-click a word block edits its source line in place', async ({ page }) => {
    await page.getByTestId('graph-beat-expand-opening').click()
    await page.waitForTimeout(900)
    await page.getByTestId('graph-block-opening-0').dblclick() // 'The lights dim.'
    const editor = page.getByTestId('graph-beat-opening').locator('textarea')
    await expect(editor).toHaveValue('The lights dim.')
    await editor.fill('The lights blaze.')
    await editor.press('Enter')
    await page.waitForTimeout(900)
    await expect(page.getByTestId('graph-beat-opening')).toContainText('The lights blaze.')
  })

  test('Add choice… appends a `* text` option through the card menu', async ({ page }) => {
    page.on('dialog', (d) => void d.accept('Jump out the window'))
    await page.getByTestId('graph-beat-opening').click({ button: 'right' })
    await page.getByTestId('graph-menu-add-choice').click()
    await page.waitForTimeout(900)
    await page.getByTestId('graph-beat-expand-opening').click()
    await page.waitForTimeout(900)
    await expect(page.getByTestId('graph-beat-opening')).toContainText('Jump out the window')
  })
})
