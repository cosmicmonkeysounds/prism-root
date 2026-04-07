import { test, expect } from "@playwright/test";

test.describe("Trust Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-trust"]').click();
    await expect(page.locator('[data-testid="tab-trust"]')).toBeVisible();
  });

  test("trust panel renders", async ({ page }) => {
    await expect(page.locator('[data-testid="trust-panel"]')).toBeVisible();
    await expect(page.locator("text=Trust & Safety").first()).toBeVisible();
  });

  test("tabs are visible", async ({ page }) => {
    await expect(page.locator('[data-testid="trust-tab-peers"]')).toBeVisible();
    await expect(page.locator('[data-testid="trust-tab-validation"]')).toBeVisible();
    await expect(page.locator('[data-testid="trust-tab-flags"]')).toBeVisible();
    await expect(page.locator('[data-testid="trust-tab-escrow"]')).toBeVisible();
  });

  test("add a peer", async ({ page }) => {
    await page.locator('[data-testid="add-peer-input"]').fill("peer-alpha");
    await page.locator('[data-testid="add-peer-btn"]').click();

    const card = page.locator('[data-testid="peer-peer-alpha"]');
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=peer-alpha")).toBeVisible();
  });

  test("trust and distrust a peer", async ({ page }) => {
    await page.locator('[data-testid="add-peer-input"]').fill("peer-beta");
    await page.locator('[data-testid="add-peer-btn"]').click();

    const card = page.locator('[data-testid="peer-peer-beta"]');
    await expect(card).toBeVisible({ timeout: 3000 });

    // Trust (+)
    await card.locator('[data-testid="trust-peer-beta"]').click();
    // Distrust (-)
    await card.locator('[data-testid="distrust-peer-beta"]').click();
  });

  test("ban a peer", async ({ page }) => {
    await page.locator('[data-testid="add-peer-input"]').fill("peer-bad");
    await page.locator('[data-testid="add-peer-btn"]').click();

    const card = page.locator('[data-testid="peer-peer-bad"]');
    await expect(card).toBeVisible({ timeout: 3000 });

    await card.locator('[data-testid="ban-peer-bad"]').click();
    await expect(card.locator("text=Banned")).toBeVisible({ timeout: 3000 });

    // Unban
    await card.locator('[data-testid="unban-peer-bad"]').click();
    await expect(card.locator("text=Banned")).not.toBeVisible({ timeout: 3000 });
  });

  test("validate JSON in validation tab", async ({ page }) => {
    await page.locator('[data-testid="trust-tab-validation"]').click();
    await page.locator('[data-testid="validate-json-input"]').fill('{"name": "test", "value": 42}');
    await page.locator('[data-testid="validate-btn"]').click();

    await expect(page.locator('[data-testid="validation-result"]')).toBeVisible({ timeout: 3000 });
    await expect(page.locator("text=Valid")).toBeVisible();
  });

  test("flag content in flags tab", async ({ page }) => {
    await page.locator('[data-testid="trust-tab-flags"]').click();
    await page.locator('[data-testid="flag-hash-input"]').fill("abc123deadbeef");
    await page.locator('[data-testid="flag-content-btn"]').click();

    const flagged = page.locator('[data-testid^="flagged-"]').first();
    await expect(flagged).toBeVisible({ timeout: 3000 });
    await expect(flagged.locator("text=spam")).toBeVisible();
  });

  test("escrow tab renders", async ({ page }) => {
    await page.locator('[data-testid="trust-tab-escrow"]').click();
    await expect(page.locator('[data-testid="escrow-payload-input"]')).toBeVisible();
    await expect(page.locator('[data-testid="escrow-deposit-btn"]')).toBeVisible();
  });

  test("KBar can navigate to Trust panel", async ({ page }) => {
    await page.locator('[data-testid="tab-close-trust"]').click();

    const isMac = await page.evaluate(() => /Mac|iPod|iPhone|iPad/.test(navigator.platform));
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");
    await page.locator('[role="combobox"]').type("Trust", { delay: 50 });
    await page.locator("text=Switch to Trust").first().click();
    await expect(page.locator('[data-testid="trust-panel"]')).toBeVisible({ timeout: 5000 });
  });
});
