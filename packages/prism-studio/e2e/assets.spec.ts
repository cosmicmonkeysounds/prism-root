import { test, expect } from "@playwright/test";

test.describe("Assets Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-assets"]').click();
    await expect(page.locator('[data-testid="tab-assets"]')).toBeVisible();
  });

  test("assets panel renders", async ({ page }) => {
    await expect(page.locator('[data-testid="assets-panel"]')).toBeVisible();
    await expect(page.locator("text=Assets").first()).toBeVisible();
  });

  test("import form is visible", async ({ page }) => {
    await expect(page.locator('[data-testid="import-asset-form"]')).toBeVisible();
    await expect(page.locator('[data-testid="import-filename-input"]')).toBeVisible();
    await expect(page.locator('[data-testid="import-data-input"]')).toBeVisible();
  });

  test("import a text file", async ({ page }) => {
    await page.locator('[data-testid="import-filename-input"]').fill("test.txt");
    await page.locator('[data-testid="import-mime-input"]').fill("text/plain");
    await page.locator('[data-testid="import-data-input"]').fill("Hello VFS");
    await page.locator('[data-testid="import-asset-btn"]').click();

    // Asset card should appear
    const card = page.locator('[data-testid^="asset-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=test.txt")).toBeVisible();
    await expect(card.locator("text=text/plain")).toBeVisible();
  });

  test("remove a file", async ({ page }) => {
    await page.locator('[data-testid="import-filename-input"]').fill("remove-me.txt");
    await page.locator('[data-testid="import-data-input"]').fill("delete this");
    await page.locator('[data-testid="import-asset-btn"]').click();

    const card = page.locator('[data-testid^="asset-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });

    await card.locator('[data-testid^="remove-asset-"]').click();
    await expect(page.locator('[data-testid^="asset-"]')).toHaveCount(0);
  });

  test("lock and unlock a file", async ({ page }) => {
    await page.locator('[data-testid="import-filename-input"]').fill("lockable.bin");
    await page.locator('[data-testid="import-data-input"]').fill("lock me");
    await page.locator('[data-testid="import-asset-btn"]').click();

    const card = page.locator('[data-testid^="asset-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });

    // Lock
    await card.locator('[data-testid^="lock-"]').click();
    await expect(card.locator("text=Locked")).toBeVisible({ timeout: 3000 });

    // Unlock
    await card.locator('[data-testid^="unlock-"]').click();
    await expect(card.locator("text=Locked")).not.toBeVisible({ timeout: 3000 });
  });

  test("notification on import", async ({ page }) => {
    await page.locator('[data-testid="import-filename-input"]').fill("notif.txt");
    await page.locator('[data-testid="import-data-input"]').fill("content");
    await page.locator('[data-testid="import-asset-btn"]').click();

    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 3000 });
  });

  test("shows file count in header", async ({ page }) => {
    await expect(page.locator("text=0 file(s)")).toBeVisible();

    await page.locator('[data-testid="import-filename-input"]').fill("count.txt");
    await page.locator('[data-testid="import-data-input"]').fill("data");
    await page.locator('[data-testid="import-asset-btn"]').click();

    await expect(page.locator("text=1 file(s)")).toBeVisible({ timeout: 3000 });
  });

  test("KBar can navigate to Assets panel", async ({ page }) => {
    await page.locator('[data-testid="tab-close-assets"]').click();

    const isMac = await page.evaluate(() => /Mac|iPod|iPhone|iPad/.test(navigator.platform));
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");
    await page.locator('[role="combobox"]').type("Assets", { delay: 50 });
    await page.locator("text=Switch to Assets").first().click();
    await expect(page.locator('[data-testid="assets-panel"]')).toBeVisible({ timeout: 5000 });
  });
});
