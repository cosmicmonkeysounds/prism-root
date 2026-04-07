import { test, expect } from "@playwright/test";

test.describe("Shortcuts Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-shortcuts"]').click();
    await expect(page.locator('[data-testid="tab-shortcuts"]')).toBeVisible();
  });

  test("shortcuts panel renders", async ({ page }) => {
    await expect(page.locator('[data-testid="shortcuts-panel"]')).toBeVisible();
    await expect(page.locator("text=Shortcuts").first()).toBeVisible();
  });

  test("default bindings are listed", async ({ page }) => {
    await expect(page.locator('[data-testid="bindings-list"]')).toBeVisible();
    // Should have default studio bindings
    await expect(page.locator("text=undo").first()).toBeVisible();
  });

  test("add binding form is visible", async ({ page }) => {
    await expect(page.locator('[data-testid="add-binding-form"]')).toBeVisible();
    await expect(page.locator('[data-testid="binding-shortcut-input"]')).toBeVisible();
    await expect(page.locator('[data-testid="binding-action-input"]')).toBeVisible();
  });

  test("add a custom binding", async ({ page }) => {
    await page.locator('[data-testid="binding-shortcut-input"]').fill("cmd+shift+t");
    await page.locator('[data-testid="binding-action-input"]').fill("test-action");
    await page.locator('[data-testid="add-binding-btn"]').click();

    await expect(page.locator("text=test-action").first()).toBeVisible();

    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 3000 });
  });

  test("remove a binding", async ({ page }) => {
    // Add one first
    await page.locator('[data-testid="binding-shortcut-input"]').fill("cmd+shift+r");
    await page.locator('[data-testid="binding-action-input"]').fill("remove-test");
    await page.locator('[data-testid="add-binding-btn"]').click();

    await expect(page.locator("text=remove-test").first()).toBeVisible();

    // Remove it
    const binding = page.locator('[data-testid="binding-cmd-shift-r"]');
    await binding.locator('[data-testid="remove-binding-cmd-shift-r"]').click();

    await expect(page.locator("text=remove-test")).toHaveCount(0);
  });

  test("switch to scopes tab", async ({ page }) => {
    await page.locator('[data-testid="shortcuts-tab-scopes"]').click();
    await expect(page.locator('[data-testid="scopes-list"]')).toBeVisible();
    // Should show the global scope
    await expect(page.locator('[data-testid="scope-global"]')).toBeVisible();
    await expect(page.locator("text=Global").first()).toBeVisible();
  });

  test("switch to events tab", async ({ page }) => {
    await page.locator('[data-testid="shortcuts-tab-events"]').click();
    await expect(page.locator('[data-testid="events-list"]')).toBeVisible();
  });

  test("KBar can navigate to Shortcuts panel", async ({ page }) => {
    await page.locator('[data-testid="tab-close-shortcuts"]').click();

    const isMac = await page.evaluate(() => /Mac|iPod|iPhone|iPad/.test(navigator.platform));
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");
    await page.locator('[role="combobox"]').type("Shortcuts", { delay: 50 });
    await page.locator("text=Switch to Shortcuts").first().click();
    await expect(page.locator('[data-testid="shortcuts-panel"]')).toBeVisible({ timeout: 5000 });
  });
});
