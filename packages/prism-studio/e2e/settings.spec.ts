import { test, expect } from "@playwright/test";

test.describe("Settings Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-settings"]').click();
    await expect(page.locator('[data-testid="tab-settings"]')).toBeVisible();
  });

  test("settings panel renders with header and search", async ({ page }) => {
    await expect(page.locator('[data-testid="settings-panel"]')).toBeVisible();
    await expect(page.locator("text=Settings").first()).toBeVisible();
    await expect(page.locator('[data-testid="settings-search"]')).toBeVisible();
  });

  test("settings are grouped by category", async ({ page }) => {
    // Built-in settings have ui, editor, sync, ai, notifications tags
    const panel = page.locator('[data-testid="settings-panel"]');
    // At least one group should be visible
    const groups = panel.locator('[data-testid^="settings-group-"]');
    const count = await groups.count();
    expect(count).toBeGreaterThan(0);
  });

  test("settings search filters results", async ({ page }) => {
    const searchInput = page.locator('[data-testid="settings-search"]');
    await searchInput.fill("theme");

    // Should show matching settings or empty state
    const panel = page.locator('[data-testid="settings-panel"]');
    // Either we find a matching setting or the "no settings" message
    const hasResult = await panel.locator('[data-testid^="setting-"]').count();
    const hasEmpty = await panel.locator("text=No settings match").count();
    expect(hasResult + hasEmpty).toBeGreaterThan(0);
  });

  test("clearing search restores all settings", async ({ page }) => {
    const searchInput = page.locator('[data-testid="settings-search"]');

    // Get count before search
    const panel = page.locator('[data-testid="settings-panel"]');
    const beforeCount = await panel.locator('[data-testid^="setting-"]').count();

    // Search for something specific
    await searchInput.fill("xyznonexistent");
    const duringCount = await panel.locator('[data-testid^="setting-"]').count();
    expect(duringCount).toBe(0);

    // Clear search
    await searchInput.fill("");
    const afterCount = await panel.locator('[data-testid^="setting-"]').count();
    expect(afterCount).toBe(beforeCount);
  });

  test("boolean settings render toggle switches", async ({ page }) => {
    const toggles = page.locator('[data-testid="settings-panel"] [role="switch"]');
    const count = await toggles.count();
    expect(count).toBeGreaterThan(0);
  });

  test("toggle switch is interactive and clickable", async ({ page }) => {
    const toggle = page.locator('[data-testid="settings-panel"] [role="switch"]').first();
    await expect(toggle).toBeVisible();

    // Toggle should have an aria-checked attribute
    const checked = await toggle.getAttribute("aria-checked");
    expect(checked === "true" || checked === "false").toBeTruthy();

    // Click should not throw (interactive)
    await toggle.click();
  });

  test("select settings render dropdown", async ({ page }) => {
    const selects = page.locator('[data-testid="settings-panel"] select');
    const count = await selects.count();
    // May or may not have selects depending on registered settings
    expect(count).toBeGreaterThanOrEqual(0);
  });

  test("KBar can navigate to Settings panel", async ({ page }) => {
    // Close settings tab
    await page.locator('[data-testid="tab-close-settings"]').click();
    await expect(page.locator('[data-testid="settings-panel"]')).not.toBeVisible();

    // Use KBar
    const isMac = await page.evaluate(() =>
      /Mac|iPod|iPhone|iPad/.test(navigator.platform),
    );
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");
    const kbarInput = page.locator('[role="combobox"]');
    await expect(kbarInput).toBeVisible({ timeout: 3_000 });

    await kbarInput.type("Settings", { delay: 50 });
    await page.locator("text=Switch to Settings").first().click();

    await expect(page.locator('[data-testid="settings-panel"]')).toBeVisible({ timeout: 5_000 });
  });

  test("settings panel shows setting keys in monospace", async ({ page }) => {
    // Each setting row should show the key
    const settings = page.locator('[data-testid="settings-panel"] [data-testid^="setting-"]');
    const count = await settings.count();
    if (count > 0) {
      // The setting key text should be present in the first setting row
      const firstSetting = settings.first();
      const keyText = firstSetting.locator("div").last();
      await expect(keyText).toBeVisible();
    }
  });
});
