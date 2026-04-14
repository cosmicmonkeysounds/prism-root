/**
 * Studio Admin Panel E2E tests.
 *
 * Exercises the admin lens: switching to it via the activity bar,
 * verifying the panel renders, and checking the source picker.
 */

import { test, expect } from "@playwright/test";

test.describe("Studio Admin Panel", () => {
  test("should switch to admin panel via activity bar", async ({ page }) => {
    await page.goto("/");

    // Wait for the shell to render
    const shell = page.locator('[data-testid="lens-shell"]');
    await expect(shell).toBeVisible();

    // Click the admin lens icon in the activity bar
    const adminIcon = page.locator('[data-testid="activity-icon-admin"]');
    await adminIcon.click();

    // Should show the admin panel
    const adminPanel = page.locator('[data-testid="admin-panel"]');
    await expect(adminPanel).toBeVisible({ timeout: 10_000 });
  });

  test("should show admin panel header with source picker", async ({ page }) => {
    await page.goto("/");

    // Navigate to admin
    await page.locator('[data-testid="activity-icon-admin"]').click();
    const adminPanel = page.locator('[data-testid="admin-panel"]');
    await expect(adminPanel).toBeVisible({ timeout: 10_000 });

    // Should show the header
    const header = page.locator('[data-testid="admin-panel-header"]');
    await expect(header).toBeVisible();
    await expect(header).toContainText("Admin Dashboard");

    // Should show the source picker dropdown
    const sourcePicker = page.locator('[data-testid="admin-source-picker"]');
    await expect(sourcePicker).toBeVisible();

    // Default source should be "Studio Kernel"
    const selectedOption = await sourcePicker.inputValue();
    expect(selectedOption).toBe("kernel");
  });

  test("should render Puck admin widgets", async ({ page }) => {
    await page.goto("/");

    // Navigate to admin
    await page.locator('[data-testid="activity-icon-admin"]').click();
    const adminPanel = page.locator('[data-testid="admin-panel"]');
    await expect(adminPanel).toBeVisible({ timeout: 10_000 });

    // The Puck editor should be loaded inside the admin panel
    // Give it time to mount the Puck components
    await page.waitForTimeout(2000);

    // The admin panel container should have content
    const panelContent = adminPanel.locator("div").first();
    await expect(panelContent).toBeVisible();
  });

  test("admin panel tab should appear in tab bar", async ({ page }) => {
    await page.goto("/");

    // Navigate to admin
    await page.locator('[data-testid="activity-icon-admin"]').click();

    // A tab for admin should appear
    const tab = page.locator('[data-testid="tab-admin"]');
    await expect(tab).toBeVisible({ timeout: 5_000 });
  });
});
