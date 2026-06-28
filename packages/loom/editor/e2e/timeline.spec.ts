import { test, expect } from '@playwright/test'

// Regression guard for the bundled-example play path + the wasm stack
// fix. With no folder open, "Start play" runs the bundled Saltmere
// example (main.loom) through the wasm engine. It must boot, play the
// opening beat, and advance into the multi-speaker `ringing` beat
// WITHOUT trapping (a too-small wasm shadow stack used to overflow
// mid-play → "memory access out of bounds").
test('bundled example plays through without the wasm trapping', async ({ page }) => {
  const errors: string[] = []
  page.on('pageerror', (e) => errors.push(String(e)))

  await page.goto('/')
  await page.getByRole('banner').getByText('Loom').waitFor()
  await page.getByTestId('mode-simulating').click()
  await page.getByRole('button', { name: 'Start play' }).click()

  // Opening beat dialogue surfaces in the transcript → engine booted.
  await expect(page.getByText(/three days/).first()).toBeVisible({ timeout: 20000 })

  // Take the choice into the ringing beat → the WREN | FISHER cue plays.
  await page.getByRole('button', { name: /Ring the bell/ }).click()
  await expect(page.getByText(/rang it/).first()).toBeVisible({ timeout: 10000 })

  expect(errors, errors.join('\n')).toEqual([])
})
