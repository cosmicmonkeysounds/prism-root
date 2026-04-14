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
    // Puck renders a component list sidebar — use first() to avoid matching activity feed text
    await expect(page.locator("text=Heading").first()).toBeVisible({ timeout: 10_000 });
    await expect(page.locator("text=Text").first()).toBeVisible();
    await expect(page.locator("text=Card").first()).toBeVisible();
  });

  test("help button toggles the DocSearch popover", async ({ page }) => {
    const helpBtn = page.locator('[data-testid="layout-panel-help-button"]');
    await expect(helpBtn).toBeVisible({ timeout: 10_000 });

    // Closed by default.
    await expect(
      page.locator('[data-testid="layout-panel-help-search"]'),
    ).toHaveCount(0);

    // Open on click.
    await helpBtn.click();
    await expect(
      page.locator('[data-testid="prism-doc-search"]'),
    ).toBeVisible();

    // Typing a query produces matching results. Scope to the DocSearch
    // popover since "Record List" also appears in Puck's own palette.
    const search = page.locator('[data-testid="prism-doc-search"]');
    const input = search.locator('input[type="text"]');
    await input.fill("record list");
    const result = search.locator('button', { hasText: "Record List" }).first();
    await expect(result).toBeVisible();

    // Clicking a result with a docPath opens DocSheet.
    await result.click();
    await expect(
      page.locator('[data-testid="prism-doc-sheet"]'),
    ).toBeVisible();
    // DocSheet renders the markdown header from record-list.md.
    await expect(page.locator('[data-testid="prism-doc-sheet"] h1').first())
      .toContainText("Record List");

    // Escape closes the sheet.
    await page.keyboard.press("Escape");
    await expect(
      page.locator('[data-testid="prism-doc-sheet"]'),
    ).toHaveCount(0);
  });
});
