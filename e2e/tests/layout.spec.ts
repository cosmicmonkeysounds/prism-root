import { test, expect } from "@playwright/test";

test.describe("Layout Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Navigate to Layout lens
    await page.locator('[data-testid="activity-icon-layout"]').click();
  });

  test("should render Puck layout builder", async ({ page }) => {
    // Puck renders its own UI with component list and canvas
    const puck = page.locator('[class*="Puck"]').first();
    await expect(puck).toBeVisible({ timeout: 10_000 });
  });

  test("should show component palette with Heading, Text, Card", async ({
    page,
  }) => {
    // Puck renders a component list sidebar
    await expect(page.locator("text=Heading")).toBeVisible({ timeout: 10_000 });
    await expect(page.locator("text=Text")).toBeVisible();
    await expect(page.locator("text=Card")).toBeVisible();
  });
});
