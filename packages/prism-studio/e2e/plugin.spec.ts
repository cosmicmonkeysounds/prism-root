import { test, expect } from "@playwright/test";

test.describe("Plugin Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-plugin"]').click();
    await expect(page.locator('[data-testid="tab-plugin"]')).toBeVisible();
  });

  test("plugin panel renders", async ({ page }) => {
    await expect(page.locator('[data-testid="plugin-panel"]')).toBeVisible();
    await expect(page.locator("text=Plugins").first()).toBeVisible();
  });

  test("register plugin form is visible", async ({ page }) => {
    await expect(page.locator('[data-testid="register-plugin-form"]')).toBeVisible();
    await expect(page.locator('[data-testid="plugin-id-input"]')).toBeVisible();
    await expect(page.locator('[data-testid="plugin-name-input"]')).toBeVisible();
  });

  test("register a plugin", async ({ page }) => {
    await page.locator('[data-testid="plugin-id-input"]').fill("my-plugin");
    await page.locator('[data-testid="plugin-name-input"]').fill("My Plugin");
    await page.locator('[data-testid="register-plugin-btn"]').click();

    const card = page.locator('[data-testid="plugin-my-plugin"]');
    await expect(card).toBeVisible();
    await expect(card.locator("text=My Plugin")).toBeVisible();
  });

  test("remove a plugin", async ({ page }) => {
    await page.locator('[data-testid="plugin-id-input"]').fill("remove-me");
    await page.locator('[data-testid="plugin-name-input"]').fill("Remove Me");
    await page.locator('[data-testid="register-plugin-btn"]').click();

    const card = page.locator('[data-testid="plugin-remove-me"]');
    await expect(card).toBeVisible();

    await card.locator('[data-testid="remove-plugin-remove-me"]').click();
    await expect(page.locator('[data-testid="plugin-remove-me"]')).toHaveCount(0);
  });

  test("expand plugin to see contributions", async ({ page }) => {
    await page.locator('[data-testid="plugin-id-input"]').fill("detail-test");
    await page.locator('[data-testid="plugin-name-input"]').fill("Detail Test");
    await page.locator('[data-testid="register-plugin-btn"]').click();

    const card = page.locator('[data-testid="plugin-detail-test"]');
    await card.locator('[data-testid="expand-plugin-detail-test"]').click();

    await expect(page.locator('[data-testid="plugin-contributions-detail-test"]')).toBeVisible();
    await expect(page.locator("text=1 command(s)")).toBeVisible();
  });

  test("switch to contributions tab", async ({ page }) => {
    // Register a plugin first
    await page.locator('[data-testid="plugin-id-input"]').fill("contrib-test");
    await page.locator('[data-testid="plugin-name-input"]').fill("Contrib Test");
    await page.locator('[data-testid="register-plugin-btn"]').click();

    await page.locator('[data-testid="plugin-tab-contributions"]').click();
    await expect(page.locator('[data-testid="plugin-contributions-list"]')).toBeVisible();
    await expect(page.locator("text=Commands").first()).toBeVisible();
  });

  test("notification on register", async ({ page }) => {
    await page.locator('[data-testid="plugin-id-input"]').fill("notif-test");
    await page.locator('[data-testid="plugin-name-input"]').fill("Notif Test");
    await page.locator('[data-testid="register-plugin-btn"]').click();

    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 3000 });
  });

  test("KBar can navigate to Plugin panel", async ({ page }) => {
    await page.locator('[data-testid="tab-close-plugin"]').click();

    const isMac = await page.evaluate(() => /Mac|iPod|iPhone|iPad/.test(navigator.platform));
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");
    await page.locator('[role="combobox"]').type("Plugins", { delay: 50 });
    await page.locator("text=Switch to Plugins").first().click();
    await expect(page.locator('[data-testid="plugin-panel"]')).toBeVisible({ timeout: 5000 });
  });
});
