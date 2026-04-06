import { test, expect } from "@playwright/test";

test.describe("Presence Indicators", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("presence indicator renders in header", async ({ page }) => {
    await expect(page.locator('[data-testid="presence-indicator"]')).toBeVisible();
  });

  test("local peer avatar is shown", async ({ page }) => {
    // The local peer should always be present
    const peers = page.locator('[data-testid^="presence-peer-"]');
    const count = await peers.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });

  test("local peer shows initial letter", async ({ page }) => {
    const peer = page.locator('[data-testid^="presence-peer-"]').first();
    const text = await peer.textContent();
    // Should be a single uppercase letter (first char of display name)
    expect(text).toMatch(/^[A-Z]$/);
  });

  test("local peer has highlighted border", async ({ page }) => {
    const peer = page.locator('[data-testid^="presence-peer-"]').first();
    const border = await peer.evaluate((el) => getComputedStyle(el).borderColor);
    // Local peer should have white border
    expect(border).toContain("255"); // rgb(255, 255, 255)
  });

  test("presence indicator has a colored avatar", async ({ page }) => {
    const peer = page.locator('[data-testid^="presence-peer-"]').first();
    const bg = await peer.evaluate((el) => getComputedStyle(el).backgroundColor);
    // Should have a non-transparent background color
    expect(bg).not.toBe("rgba(0, 0, 0, 0)");
  });
});
