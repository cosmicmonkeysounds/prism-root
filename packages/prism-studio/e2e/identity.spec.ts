import { test, expect } from "@playwright/test";

test.describe("Identity Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-identity"]').click();
    await expect(page.locator('[data-testid="tab-identity"]')).toBeVisible();
  });

  test("identity panel renders", async ({ page }) => {
    await expect(page.locator('[data-testid="identity-panel"]')).toBeVisible();
    await expect(page.locator("text=Identity").first()).toBeVisible();
  });

  test("shows no identity initially", async ({ page }) => {
    await expect(page.locator('[data-testid="identity-card"]')).toBeVisible();
    await expect(page.locator("text=No identity generated yet")).toBeVisible();
  });

  test("generate identity creates DID", async ({ page }) => {
    await page.locator('[data-testid="generate-identity-btn"]').click();
    await expect(page.locator('[data-testid="identity-did"]')).toBeVisible({ timeout: 5000 });
    const did = await page.locator('[data-testid="identity-did"]').textContent();
    expect(did).toMatch(/^did:key:z/);
  });

  test("sign and verify payload", async ({ page }) => {
    await page.locator('[data-testid="generate-identity-btn"]').click();
    await expect(page.locator('[data-testid="identity-did"]')).toBeVisible({ timeout: 5000 });

    await page.locator('[data-testid="sign-payload-input"]').fill("Hello Prism");
    await page.locator('[data-testid="sign-btn"]').click();
    await expect(page.locator('[data-testid="signature-output"]')).toBeVisible({ timeout: 5000 });

    await page.locator('[data-testid="verify-btn"]').click();
    await expect(page.locator("text=Valid")).toBeVisible({ timeout: 3000 });
  });

  test("export identity to JSON", async ({ page }) => {
    await page.locator('[data-testid="generate-identity-btn"]').click();
    await expect(page.locator('[data-testid="identity-did"]')).toBeVisible({ timeout: 5000 });

    await page.locator('[data-testid="export-identity-btn"]').click();
    const textarea = page.locator('[data-testid="import-json-input"]');
    await expect(textarea).not.toHaveValue("", { timeout: 3000 });
    const json = await textarea.inputValue();
    expect(json).toContain("did");
  });

  test("sign-verify section renders", async ({ page }) => {
    await expect(page.locator('[data-testid="sign-verify-section"]')).toBeVisible();
    await expect(page.locator('[data-testid="sign-payload-input"]')).toBeVisible();
  });

  test("export-import section renders", async ({ page }) => {
    await expect(page.locator('[data-testid="export-import-section"]')).toBeVisible();
    await expect(page.locator('[data-testid="import-json-input"]')).toBeVisible();
  });

  test("KBar can navigate to Identity panel", async ({ page }) => {
    await page.locator('[data-testid="tab-close-identity"]').click();

    const isMac = await page.evaluate(() => /Mac|iPod|iPhone|iPad/.test(navigator.platform));
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");
    await page.locator('[role="combobox"]').type("Identity", { delay: 50 });
    await page.locator("text=Switch to Identity").first().click();
    await expect(page.locator('[data-testid="identity-panel"]')).toBeVisible({ timeout: 5000 });
  });
});
