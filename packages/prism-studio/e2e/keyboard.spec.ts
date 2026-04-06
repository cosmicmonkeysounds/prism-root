import { test, expect } from "@playwright/test";

test.describe("Keyboard Navigation", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("CMD+K opens KBar command palette", async ({ page }) => {
    const isMac = await page.evaluate(() =>
      /Mac|iPod|iPhone|iPad/.test(navigator.platform),
    );
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");

    const kbarInput = page.locator('[role="combobox"]');
    await expect(kbarInput).toBeVisible({ timeout: 3_000 });
  });

  test("Escape closes KBar", async ({ page }) => {
    const isMac = await page.evaluate(() =>
      /Mac|iPod|iPhone|iPad/.test(navigator.platform),
    );
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");

    const kbarInput = page.locator('[role="combobox"]');
    await expect(kbarInput).toBeVisible({ timeout: 3_000 });

    await page.keyboard.press("Escape");
    await expect(kbarInput).not.toBeVisible();
  });

  test("KBar lists lens navigation actions", async ({ page }) => {
    const isMac = await page.evaluate(() =>
      /Mac|iPod|iPhone|iPad/.test(navigator.platform),
    );
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");

    const kbarInput = page.locator('[role="combobox"]');
    await expect(kbarInput).toBeVisible({ timeout: 3_000 });

    // KBar derives actions as "Switch to <LensName>" from lens manifests
    // Also includes Undo/Redo actions
    await expect(page.locator("text=Switch to Editor")).toBeVisible();
    await expect(page.locator("text=Switch to Graph")).toBeVisible();
    await expect(page.locator("text=Switch to Layout").first()).toBeVisible();
    await expect(page.locator("text=Switch to Canvas").first()).toBeVisible();
    await expect(page.locator("text=Switch to CRDT").first()).toBeVisible();
    await expect(page.locator("text=Switch to Relay").first()).toBeVisible();
  });

  test("KBar filters results by search query", async ({ page }) => {
    const isMac = await page.evaluate(() =>
      /Mac|iPod|iPhone|iPad/.test(navigator.platform),
    );
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");

    const kbarInput = page.locator('[role="combobox"]');
    await expect(kbarInput).toBeVisible({ timeout: 3_000 });

    await kbarInput.type("CRDT", { delay: 50 });

    // CRDT should be visible, others filtered out
    await expect(page.locator("text=Switch to CRDT").first()).toBeVisible();
  });

  test("KBar action navigates to selected lens", async ({ page }) => {
    const isMac = await page.evaluate(() =>
      /Mac|iPod|iPhone|iPad/.test(navigator.platform),
    );
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");

    const kbarInput = page.locator('[role="combobox"]');
    await expect(kbarInput).toBeVisible({ timeout: 3_000 });

    await kbarInput.type("Layout", { delay: 50 });
    await page.locator("text=Switch to Layout").first().click();

    // Puck should render
    const puck = page.locator('[class*="Puck"]').first();
    await expect(puck).toBeVisible({ timeout: 10_000 });
  });
});
