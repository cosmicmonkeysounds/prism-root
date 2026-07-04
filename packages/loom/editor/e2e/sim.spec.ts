// Sim mode end-to-end: start the local simulator, act as a persona
// making choices, fire enumerated named events from the header, and
// watch the runtime overlay light the story map.

import { test, expect } from '@playwright/test'
import { openProject, switchMode } from './helpers'

test.describe('sim mode', () => {
  test.beforeEach(async ({ page }) => {
    await openProject(page)
    await switchMode(page, 'sim')
    await page.getByTestId('sim-start').click()
    await page.waitForTimeout(500)
  })

  test('start creates a persona, fires the entry beat, and offers its choice', async ({ page }) => {
    await expect(page.getByTestId('sim-persona-p1')).toBeVisible()
    // The entry menu suspended unbound → the global choice card.
    await expect(page.getByTestId('choice-__global-0')).toContainText('Take the stairs')
    await expect(page.getByTestId('choice-__global-1')).toContainText('Take the lift')
  })

  test('answering a choice resumes the story into the chosen beat', async ({ page }) => {
    await page.getByTestId('choice-__global-0').click()
    await page.waitForTimeout(300)
    await expect(page.getByTestId('choice-__global-0')).toHaveCount(0)

    await page.getByRole('tab', { name: 'Log' }).click()
    const log = page.getByTestId('sim-log')
    await expect(log).toContainText('== opening')
    await expect(log).toContainText('== stairs')
    await expect(log).toContainText('Welcome, traveler.')
  })

  test('the story map lights up with visits from the local run', async ({ page }) => {
    await page.getByTestId('choice-__global-1').click()
    await page.getByRole('tab', { name: 'Story' }).click()
    await expect(page.getByTestId('graph-beat-lift')).toBeVisible()
    await page.waitForTimeout(1200) // layout settles
    // Visit badges on the beats the run entered.
    await expect(page.getByTestId('graph-beat-opening')).toContainText('1')
    await expect(page.getByTestId('graph-beat-lift')).toContainText('1')
  })

  test('quick-fire enumerates named events and firing one reaches its room', async ({ page }) => {
    const select = page.getByTestId('sim-quickfire-select')
    await expect(select).toBeVisible()
    await expect(select.locator('option', { hasText: 'lockdown' })).toHaveCount(1)

    await select.selectOption('lockdown')
    await page.getByTestId('sim-quickfire-button').click()
    await page.waitForTimeout(300)

    // The broadcast lands in the faction room via the rooms rail.
    await page.getByRole('button', { name: /#mods/ }).click()
    await expect(page.getByText('Lockdown — the Algorithm tightens its grip.')).toBeVisible()
  })

  test('adding a persona and scanning it fires the character hook', async ({ page }) => {
    await page.getByTestId('sim-persona-name').fill('Beta')
    await page.getByTestId('sim-persona-add').click()
    await expect(page.getByTestId('sim-persona-p2')).toBeVisible()

    // Open its Inspector and scan as the Greeter → +5 score.
    await page.getByTestId('sim-persona-p2').click()
    const tray = page.locator('select', { hasText: 'scan as character' }).first()
    await tray.selectOption('Greeter')
    await page.getByRole('button', { name: 'Scan', exact: true }).click()
    await page.waitForTimeout(300)
    await page.getByRole('tab', { name: 'Roster' }).click()
    const row = page.getByRole('row').filter({ hasText: 'Beta' })
    await expect(row).toContainText('5')
  })

  test('stop clears the session', async ({ page }) => {
    await page.getByRole('button', { name: '■ Stop' }).click()
    await expect(page.getByTestId('sim-start')).toBeVisible()
    await expect(page.getByTestId('sim-persona-p1')).toHaveCount(0)
  })
})
