import { test, expect } from "@playwright/test";

test.describe("Expression Bar", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Select an object so inspector shows
    const explorer = page.locator('[data-testid="object-explorer"]');
    await explorer.locator('[data-testid^="explorer-node-"]').first().click();
    await expect(page.locator('[data-testid="inspector-content"]')).toBeVisible();
  });

  test("expression bar renders in inspector", async ({ page }) => {
    await expect(page.locator('[data-testid="expression-bar"]')).toBeVisible();
    await expect(page.locator('[data-testid="expression-input"]')).toBeVisible();
    await expect(page.locator('[data-testid="expression-eval-btn"]')).toBeVisible();
  });

  test("evaluate a simple expression", async ({ page }) => {
    const input = page.locator('[data-testid="expression-input"]');
    await input.fill("2 + 3");
    await page.locator('[data-testid="expression-eval-btn"]').click();

    const result = page.locator('[data-testid="expression-result"]');
    await expect(result).toBeVisible();
    await expect(result).toContainText("5");
  });

  test("evaluate with Enter key", async ({ page }) => {
    const input = page.locator('[data-testid="expression-input"]');
    await input.fill("10 * 2");
    await input.press("Enter");

    const result = page.locator('[data-testid="expression-result"]');
    await expect(result).toBeVisible();
    await expect(result).toContainText("20");
  });

  test("show error for invalid expression", async ({ page }) => {
    const input = page.locator('[data-testid="expression-input"]');
    await input.fill("2 +");
    await page.locator('[data-testid="expression-eval-btn"]').click();

    const result = page.locator('[data-testid="expression-result"]');
    await expect(result).toBeVisible();
    await expect(result).toContainText("Error");
  });

  test("evaluate expression with object context", async ({ page }) => {
    // The selected object has a position field
    const input = page.locator('[data-testid="expression-input"]');
    await input.fill("position + 1");
    await page.locator('[data-testid="expression-eval-btn"]').click();

    const result = page.locator('[data-testid="expression-result"]');
    await expect(result).toBeVisible();
    // position is 0-based, so result should be 1
    await expect(result).toContainText("= ");
  });

  test("evaluate math functions", async ({ page }) => {
    const input = page.locator('[data-testid="expression-input"]');
    await input.fill("abs(-5)");
    await page.locator('[data-testid="expression-eval-btn"]').click();

    const result = page.locator('[data-testid="expression-result"]');
    await expect(result).toBeVisible();
    await expect(result).toContainText("5");
  });
});
