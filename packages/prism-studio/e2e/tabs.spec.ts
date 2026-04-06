import { test, expect } from "@playwright/test";

test.describe("Tab Management", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("opening the same lens twice deduplicates (singleton tabs)", async ({
    page,
  }) => {
    // Editor is already open
    await expect(page.locator('[data-testid="tab-editor"]')).toBeVisible();

    // Click editor icon again — should not create a second tab
    await page.locator('[data-testid="activity-icon-editor"]').click();

    const editorTabs = page.locator('[data-testid="tab-editor"]');
    await expect(editorTabs).toHaveCount(1);
  });

  test("opening all 9 lenses shows 9 tabs", async ({ page }) => {
    // Editor is open by default
    await page.locator('[data-testid="activity-icon-graph"]').click();
    await expect(page.locator('[data-testid="tab-graph"]')).toBeAttached();

    await page.locator('[data-testid="activity-icon-layout"]').click();
    await expect(page.locator('[data-testid="tab-layout"]')).toBeAttached();

    await page.locator('[data-testid="activity-icon-canvas"]').click();
    await expect(page.locator('[data-testid="tab-canvas"]')).toBeAttached();

    await page.locator('[data-testid="activity-icon-crdt"]').click();
    await expect(page.locator('[data-testid="tab-crdt"]')).toBeAttached();

    await page.locator('[data-testid="activity-icon-relay"]').click();
    await expect(page.locator('[data-testid="tab-relay"]')).toBeAttached();

    await page.locator('[data-testid="activity-icon-settings"]').click();
    await expect(page.locator('[data-testid="tab-settings"]')).toBeAttached();

    await page.locator('[data-testid="activity-icon-automation"]').click();
    await expect(page.locator('[data-testid="tab-automation"]')).toBeAttached();

    await page.locator('[data-testid="activity-icon-analysis"]').click();
    await expect(page.locator('[data-testid="tab-analysis"]')).toBeAttached();

    // All 9 lens tabs should be in the DOM
    const tabBar = page.locator('[data-testid="tab-bar"]');
    const tabs = tabBar.locator(':scope > [data-testid^="tab-"]');
    await expect(tabs).toHaveCount(9);
  });

  test("closing all tabs shows empty state", async ({ page }) => {
    // Close the default editor tab
    await page.locator('[data-testid="tab-close-editor"]').click();

    // Should show the "No tab open" message
    await expect(
      page.locator("text=No tab open. Click a lens in the activity bar."),
    ).toBeVisible();
  });

  test("closing active tab activates previous tab", async ({ page }) => {
    // Open graph tab (editor is already open)
    await page.locator('[data-testid="activity-icon-graph"]').click();
    await expect(page.locator(".react-flow")).toBeVisible({ timeout: 10_000 });

    // Close graph tab — should fall back to editor
    await page.locator('[data-testid="tab-close-graph"]').click();
    await expect(page.locator("text=editor_content (LoroText)")).toBeVisible({
      timeout: 5_000,
    });
  });

  test("pinning and unpinning a tab toggles pinned state", async ({
    page,
  }) => {
    // Pin the editor tab
    await page.locator('[data-testid="tab-pin-editor"]').click();
    await expect(page.locator('[data-testid="tab-editor"]')).toHaveAttribute(
      "data-pinned",
      "true",
    );

    // Unpin it
    await page.locator('[data-testid="tab-pin-editor"]').click();
    await expect(page.locator('[data-testid="tab-editor"]')).toHaveAttribute(
      "data-pinned",
      "false",
    );
  });
});
