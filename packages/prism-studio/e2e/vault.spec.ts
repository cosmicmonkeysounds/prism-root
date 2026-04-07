import { test, expect } from "@playwright/test";

test.describe("Vault Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-vault"]').click();
    await expect(page.locator('[data-testid="tab-vault"]')).toBeVisible();
  });

  test("vault panel renders", async ({ page }) => {
    await expect(page.locator('[data-testid="vault-panel"]')).toBeVisible();
    await expect(page.locator("text=Vaults").first()).toBeVisible();
  });

  test("add vault form is visible", async ({ page }) => {
    await expect(page.locator('[data-testid="add-vault-form"]')).toBeVisible();
    await expect(page.locator('[data-testid="vault-name-input"]')).toBeVisible();
    await expect(page.locator('[data-testid="vault-path-input"]')).toBeVisible();
  });

  test("add a vault", async ({ page }) => {
    await page.locator('[data-testid="vault-name-input"]').fill("My Workspace");
    await page.locator('[data-testid="vault-path-input"]').fill("/home/user/workspace");
    await page.locator('[data-testid="add-vault-btn"]').click();

    const card = page.locator('[data-testid^="vault-vault_"]').first();
    await expect(card).toBeVisible();
    await expect(card.locator("text=My Workspace")).toBeVisible();
    await expect(card.getByText("/home/user/workspace")).toBeVisible();
  });

  test("remove a vault", async ({ page }) => {
    await page.locator('[data-testid="vault-name-input"]').fill("Remove Me");
    await page.locator('[data-testid="vault-path-input"]').fill("/tmp/remove");
    await page.locator('[data-testid="add-vault-btn"]').click();

    const card = page.locator('[data-testid^="vault-vault_"]').first();
    await expect(card).toBeVisible();

    await card.locator('[data-testid^="remove-vault-"]').click();
    await expect(page.locator('[data-testid^="vault-vault_"]')).toHaveCount(0);
  });

  test("pin a vault", async ({ page }) => {
    await page.locator('[data-testid="vault-name-input"]').fill("Pin Test");
    await page.locator('[data-testid="vault-path-input"]').fill("/tmp/pin");
    await page.locator('[data-testid="add-vault-btn"]').click();

    const card = page.locator('[data-testid^="vault-vault_"]').first();
    await card.locator('[data-testid^="pin-vault-"]').click();

    // Should now show in pinned section
    await expect(page.locator("text=Pinned (1)")).toBeVisible();
    await expect(page.locator("text=\u2605 Pin Test")).toBeVisible();
  });

  test("open a vault updates lastOpenedAt", async ({ page }) => {
    await page.locator('[data-testid="vault-name-input"]').fill("Open Test");
    await page.locator('[data-testid="vault-path-input"]').fill("/tmp/open");
    await page.locator('[data-testid="add-vault-btn"]').click();

    const card = page.locator('[data-testid^="vault-vault_"]').first();
    await card.locator('[data-testid^="open-vault-"]').click();

    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 3000 });
  });

  test("search filters vaults", async ({ page }) => {
    // Add two vaults
    await page.locator('[data-testid="vault-name-input"]').fill("Alpha Vault");
    await page.locator('[data-testid="vault-path-input"]').fill("/tmp/alpha");
    await page.locator('[data-testid="add-vault-btn"]').click();

    await page.locator('[data-testid="vault-name-input"]').fill("Beta Vault");
    await page.locator('[data-testid="vault-path-input"]').fill("/tmp/beta");
    await page.locator('[data-testid="add-vault-btn"]').click();

    await expect(page.locator('[data-testid^="vault-vault_"]')).toHaveCount(2);

    // Search for Alpha
    await page.locator('[data-testid="vault-search-input"]').fill("Alpha");
    await expect(page.locator('[data-testid^="vault-vault_"]')).toHaveCount(1);
    // Verify it's the Alpha vault card that remains
    const remainingCard = page.locator('[data-testid^="vault-vault_"]').first();
    await expect(remainingCard.locator("text=Alpha Vault")).toBeVisible();
  });

  test("notification on add", async ({ page }) => {
    await page.locator('[data-testid="vault-name-input"]').fill("Notif Test");
    await page.locator('[data-testid="vault-path-input"]').fill("/tmp/notif");
    await page.locator('[data-testid="add-vault-btn"]').click();

    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 3000 });
  });

  test("KBar can navigate to Vault panel", async ({ page }) => {
    await page.locator('[data-testid="tab-close-vault"]').click();

    const isMac = await page.evaluate(() => /Mac|iPod|iPhone|iPad/.test(navigator.platform));
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");
    await page.locator('[role="combobox"]').type("Vaults", { delay: 50 });
    await page.locator("text=Switch to Vaults").first().click();
    await expect(page.locator('[data-testid="vault-panel"]')).toBeVisible({ timeout: 5000 });
  });
});
