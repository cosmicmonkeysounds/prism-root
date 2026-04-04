import { test, expect } from "@playwright/test";

test.describe("Phase 4: The Shell", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("activity bar renders with all lens icons", async ({ page }) => {
    const activityBar = page.locator('[data-testid="activity-bar"]');
    await expect(activityBar).toBeVisible();
    const icons = activityBar.locator('[data-testid^="activity-icon-"]');
    await expect(icons).toHaveCount(4);
  });

  test("clicking activity bar icon opens a tab", async ({ page }) => {
    await page.locator('[data-testid="activity-icon-graph"]').click();
    const tab = page.locator('[data-testid="tab-graph"]');
    await expect(tab).toBeVisible();
  });

  test("tab bar shows open tabs", async ({ page }) => {
    const tabBar = page.locator('[data-testid="tab-bar"]');
    await expect(tabBar).toBeVisible();
    // Default: editor tab is open
    await expect(
      tabBar.locator('[data-testid^="tab-"]').first(),
    ).toBeVisible();
  });

  test("close tab removes it from tab bar", async ({ page }) => {
    // Open graph tab
    await page.locator('[data-testid="activity-icon-graph"]').click();
    await expect(page.locator('[data-testid="tab-graph"]')).toBeVisible();
    // Close it
    await page.locator('[data-testid="tab-close-graph"]').click();
    await expect(page.locator('[data-testid="tab-graph"]')).not.toBeVisible();
  });

  test("pin tab shows pin indicator", async ({ page }) => {
    // Pin the default editor tab
    await page.locator('[data-testid="tab-pin-editor"]').click();
    await expect(page.locator('[data-testid="tab-editor"]')).toHaveAttribute(
      "data-pinned",
      "true",
    );
  });

  test("sidebar toggle shows/hides sidebar", async ({ page }) => {
    const sidebar = page.locator('[data-testid="sidebar"]');
    // Sidebar is visible by default
    await expect(sidebar).toBeVisible();
    // Toggle off
    await page.locator('[data-testid="toggle-sidebar"]').click();
    await expect(sidebar).not.toBeVisible();
    // Toggle on (button is in the header when sidebar is hidden)
    await page.locator('[data-testid="toggle-sidebar"]').click();
    await expect(sidebar).toBeVisible();
  });

  test("KBar navigation still works", async ({ page }) => {
    const isMac = await page.evaluate(() =>
      /Mac|iPod|iPhone|iPad/.test(navigator.platform),
    );
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");
    const kbarInput = page.locator('[role="combobox"]');
    await expect(kbarInput).toBeVisible({ timeout: 3_000 });
    await kbarInput.type("Graph", { delay: 50 });
    const result = page.locator("text=Switch to Graph");
    await expect(result).toBeVisible({ timeout: 3_000 });
    await result.click();
    const flow = page.locator(".react-flow");
    await expect(flow).toBeVisible({ timeout: 10_000 });
  });

  test("multiple tabs can be open and switched between", async ({ page }) => {
    // Editor is open by default; open graph
    await page.locator('[data-testid="activity-icon-graph"]').click();
    await expect(page.locator('[data-testid="tab-graph"]')).toBeVisible();
    // Graph content should be showing
    await expect(page.locator(".react-flow")).toBeVisible({ timeout: 10_000 });
    // Switch back to editor tab by clicking the activity bar icon
    await page.locator('[data-testid="activity-icon-editor"]').click();
    // Editor content should be visible (CodeMirror editor area)
    await expect(page.locator("text=editor_content (LoroText)")).toBeVisible({
      timeout: 5_000,
    });
  });
});
