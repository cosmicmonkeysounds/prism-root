import { test, expect } from "@playwright/test";

test.describe("Analysis Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-analysis"]').click();
    await expect(page.locator('[data-testid="tab-analysis"]')).toBeVisible();
  });

  test("analysis panel renders", async ({ page }) => {
    await expect(page.locator('[data-testid="analysis-panel"]')).toBeVisible();
    await expect(page.locator("text=Analysis").first()).toBeVisible();
  });

  test("tab buttons are visible", async ({ page }) => {
    await expect(page.locator('[data-testid="analysis-tab-plan"]')).toBeVisible();
    await expect(page.locator('[data-testid="analysis-tab-cycles"]')).toBeVisible();
    await expect(page.locator('[data-testid="analysis-tab-impact"]')).toBeVisible();
  });

  test("critical path tab shows plan data", async ({ page }) => {
    await page.locator('[data-testid="analysis-tab-plan"]').click();
    await expect(page.locator('[data-testid="analysis-plan"]')).toBeVisible();
  });

  test("cycles tab shows cycle detection", async ({ page }) => {
    await page.locator('[data-testid="analysis-tab-cycles"]').click();
    await expect(page.locator('[data-testid="analysis-cycles"]')).toBeVisible();
    // With seed data (no dependency cycles), should show green message
    await expect(page.locator("text=No dependency cycles detected")).toBeVisible();
  });

  test("impact tab shows impact analysis", async ({ page }) => {
    await page.locator('[data-testid="analysis-tab-impact"]').click();
    await expect(page.locator('[data-testid="analysis-impact"]')).toBeVisible();
  });

  test("impact tab shows analysis for selection", async ({ page }) => {
    await page.locator('[data-testid="analysis-tab-impact"]').click();
    const impactSection = page.locator('[data-testid="analysis-impact"]');
    await expect(impactSection).toBeVisible();
  });

  test("selecting an object shows impact details", async ({ page }) => {
    // Select an object first via object explorer
    const explorer = page.locator('[data-testid="object-explorer"]');
    await explorer.locator('[data-testid^="explorer-node-"]').first().click();

    // Switch to impact tab
    await page.locator('[data-testid="analysis-tab-impact"]').click();
    await expect(page.locator("text=Analyzing:")).toBeVisible();
    await expect(page.locator("text=Blocking Chain").first()).toBeVisible();
    await expect(page.locator("text=Downstream Impact").first()).toBeVisible();
    await expect(page.locator("text=Slip Impact Calculator").first()).toBeVisible();
  });

  test("slip days input is interactive", async ({ page }) => {
    // Select an object
    const explorer = page.locator('[data-testid="object-explorer"]');
    await explorer.locator('[data-testid^="explorer-node-"]').first().click();

    await page.locator('[data-testid="analysis-tab-impact"]').click();
    const slipInput = page.locator('[data-testid="slip-days-input"]');
    await expect(slipInput).toBeVisible();

    await slipInput.fill("5");
    await expect(slipInput).toHaveValue("5");
  });

  test("KBar can navigate to Analysis panel", async ({ page }) => {
    await page.locator('[data-testid="tab-close-analysis"]').click();

    const isMac = await page.evaluate(() => /Mac|iPod|iPhone|iPad/.test(navigator.platform));
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");
    await page.locator('[role="combobox"]').type("Analysis", { delay: 50 });
    await page.locator("text=Switch to Analysis").first().click();
    await expect(page.locator('[data-testid="analysis-panel"]')).toBeVisible({ timeout: 5000 });
  });
});
