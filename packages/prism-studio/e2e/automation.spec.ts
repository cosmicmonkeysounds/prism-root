import { test, expect } from "@playwright/test";

test.describe("Automation Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-automation"]').click();
    await expect(page.locator('[data-testid="tab-automation"]')).toBeVisible();
  });

  test("automation panel renders", async ({ page }) => {
    await expect(page.locator('[data-testid="automation-panel"]')).toBeVisible();
    await expect(page.locator("text=Automation").first()).toBeVisible();
  });

  test("create automation form is visible", async ({ page }) => {
    await expect(page.locator('[data-testid="create-automation-form"]')).toBeVisible();
    await expect(page.locator('[data-testid="automation-name-input"]')).toBeVisible();
    await expect(page.locator('[data-testid="automation-trigger-select"]')).toBeVisible();
    await expect(page.locator('[data-testid="automation-action-select"]')).toBeVisible();
  });

  test("create a manual automation", async ({ page }) => {
    await page.locator('[data-testid="automation-name-input"]').fill("Test Rule");
    await page.locator('[data-testid="create-automation-btn"]').click();

    // Should appear in the list
    const card = page.locator('[data-testid^="automation-auto_"]').first();
    await expect(card).toBeVisible();
    await expect(card.locator("text=Test Rule")).toBeVisible();
    await expect(card.locator("text=Manual")).toBeVisible();
    await expect(card.locator("text=enabled")).toBeVisible();
  });

  test("toggle automation enabled/disabled", async ({ page }) => {
    await page.locator('[data-testid="automation-name-input"]').fill("Toggle Test");
    await page.locator('[data-testid="create-automation-btn"]').click();

    const card = page.locator('[data-testid^="automation-auto_"]').first();
    await expect(card.locator("text=enabled")).toBeVisible();

    // Disable
    await card.locator('[data-testid^="toggle-automation-"]').click();
    await expect(card.locator("text=disabled")).toBeVisible();

    // Re-enable
    await card.locator('[data-testid^="toggle-automation-"]').click();
    await expect(card.locator("text=enabled")).toBeVisible();
  });

  test("run automation manually", async ({ page }) => {
    await page.locator('[data-testid="automation-name-input"]').fill("Run Test");
    await page.locator('[data-testid="create-automation-btn"]').click();

    const card = page.locator('[data-testid^="automation-auto_"]').first();
    await card.locator('[data-testid^="run-automation-"]').click();

    // Should show notification toast
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 3000 });
  });

  test("delete automation", async ({ page }) => {
    await page.locator('[data-testid="automation-name-input"]').fill("Delete Test");
    await page.locator('[data-testid="create-automation-btn"]').click();

    const card = page.locator('[data-testid^="automation-auto_"]').first();
    await expect(card).toBeVisible();

    await card.locator('[data-testid^="delete-automation-"]').click();
    await expect(page.locator('[data-testid^="automation-auto_"]')).toHaveCount(0);
  });

  test("switch between rules and history tabs", async ({ page }) => {
    await expect(page.locator('[data-testid="automation-tab-rules"]')).toBeVisible();
    await expect(page.locator('[data-testid="automation-tab-history"]')).toBeVisible();

    // Switch to history
    await page.locator('[data-testid="automation-tab-history"]').click();
    await expect(page.locator('[data-testid="automation-history"]')).toBeVisible();

    // Switch back to rules
    await page.locator('[data-testid="automation-tab-rules"]').click();
    await expect(page.locator('[data-testid="create-automation-form"]')).toBeVisible();
  });

  test("automation run appears in history", async ({ page }) => {
    // Create and run
    await page.locator('[data-testid="automation-name-input"]').fill("History Test");
    await page.locator('[data-testid="create-automation-btn"]').click();

    const card = page.locator('[data-testid^="automation-auto_"]').first();
    await card.locator('[data-testid^="run-automation-"]').click();
    // Wait for run to complete
    await page.waitForTimeout(500);

    // Switch to history tab
    await page.locator('[data-testid="automation-tab-history"]').click();
    const history = page.locator('[data-testid="automation-history"]');
    await expect(history).toBeVisible();

    // Should have at least one run entry
    await expect(history.locator("text=History Test")).toBeVisible({ timeout: 3000 });
  });

  test("trigger type selection works", async ({ page }) => {
    const select = page.locator('[data-testid="automation-trigger-select"]');
    await select.selectOption("object:created");
    await expect(select).toHaveValue("object:created");

    await select.selectOption("cron");
    await expect(select).toHaveValue("cron");
  });

  test("KBar can navigate to Automation panel", async ({ page }) => {
    // Close the automation tab first
    await page.locator('[data-testid="tab-close-automation"]').click();

    const isMac = await page.evaluate(() => /Mac|iPod|iPhone|iPad/.test(navigator.platform));
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");
    await page.locator('[role="combobox"]').type("Automation", { delay: 50 });
    await page.locator("text=Switch to Automation").first().click();
    await expect(page.locator('[data-testid="automation-panel"]')).toBeVisible({ timeout: 5000 });
  });
});
