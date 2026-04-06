import { test, expect } from "@playwright/test";

test.describe("View Mode Switcher", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Ensure sidebar is visible with the object explorer
    await expect(page.locator('[data-testid="sidebar"]')).toBeVisible();
  });

  test("view mode switcher renders in explorer header", async ({ page }) => {
    await expect(page.locator('[data-testid="view-mode-switcher"]')).toBeVisible();
  });

  test("four view mode buttons are present", async ({ page }) => {
    const switcher = page.locator('[data-testid="view-mode-switcher"]');
    await expect(switcher.locator('[data-testid="view-mode-list"]')).toBeVisible();
    await expect(switcher.locator('[data-testid="view-mode-kanban"]')).toBeVisible();
    await expect(switcher.locator('[data-testid="view-mode-grid"]')).toBeVisible();
    await expect(switcher.locator('[data-testid="view-mode-table"]')).toBeVisible();
  });

  test("list view is the default mode", async ({ page }) => {
    // List mode button should have the active style (background #094771)
    const listBtn = page.locator('[data-testid="view-mode-list"]');
    const bg = await listBtn.evaluate((el) => getComputedStyle(el).backgroundColor);
    // #094771 = rgb(9, 71, 113)
    expect(bg).toContain("9");
  });

  test("switching to kanban view shows columns", async ({ page }) => {
    // First create an object so we have something to show
    await page.locator('[data-testid="create-page-btn"]').click();

    // Switch to kanban
    await page.locator('[data-testid="view-mode-kanban"]').click();
    await expect(page.locator('[data-testid="kanban-view"]')).toBeVisible();

    // Should have at least one column
    const columns = page.locator('[data-testid^="kanban-column-"]');
    const count = await columns.count();
    expect(count).toBeGreaterThan(0);
  });

  test("kanban cards are clickable to select", async ({ page }) => {
    await page.locator('[data-testid="create-page-btn"]').click();
    await page.locator('[data-testid="view-mode-kanban"]').click();

    const card = page.locator('[data-testid^="kanban-card-"]').first();
    await expect(card).toBeVisible();
    await card.click();

    // Card should now have selected styling
    const bg = await card.evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(bg).toContain("9"); // #094771
  });

  test("switching to grid view shows card tiles", async ({ page }) => {
    await page.locator('[data-testid="create-page-btn"]').click();
    await page.locator('[data-testid="view-mode-grid"]').click();

    await expect(page.locator('[data-testid="grid-view"]')).toBeVisible();
    const cards = page.locator('[data-testid^="grid-card-"]');
    const count = await cards.count();
    expect(count).toBeGreaterThan(0);
  });

  test("grid cards are clickable to select", async ({ page }) => {
    await page.locator('[data-testid="create-page-btn"]').click();
    await page.locator('[data-testid="view-mode-grid"]').click();

    const card = page.locator('[data-testid^="grid-card-"]').first();
    await card.click();

    const bg = await card.evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(bg).toContain("9"); // #094771
  });

  test("switching to table view shows rows", async ({ page }) => {
    await page.locator('[data-testid="create-page-btn"]').click();
    await page.locator('[data-testid="view-mode-table"]').click();

    await expect(page.locator('[data-testid="table-view"]')).toBeVisible();
    const rows = page.locator('[data-testid^="table-row-"]');
    const count = await rows.count();
    expect(count).toBeGreaterThan(0);
  });

  test("table shows name, type, and status columns", async ({ page }) => {
    await page.locator('[data-testid="create-page-btn"]').click();
    await page.locator('[data-testid="view-mode-table"]').click();

    const table = page.locator('[data-testid="table-view"]');
    await expect(table.locator("th", { hasText: "Name" })).toBeVisible();
    await expect(table.locator("th", { hasText: "Type" })).toBeVisible();
    await expect(table.locator("th", { hasText: "Status" })).toBeVisible();
  });

  test("table rows are clickable to select", async ({ page }) => {
    await page.locator('[data-testid="create-page-btn"]').click();
    await page.locator('[data-testid="view-mode-table"]').click();

    const row = page.locator('[data-testid^="table-row-"]').first();
    await row.click();

    const bg = await row.evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(bg).toContain("9"); // #094771
  });

  test("switching back to list view restores tree", async ({ page }) => {
    await page.locator('[data-testid="create-page-btn"]').click();

    // Switch to grid, then back to list
    await page.locator('[data-testid="view-mode-grid"]').click();
    await expect(page.locator('[data-testid="grid-view"]')).toBeVisible();

    await page.locator('[data-testid="view-mode-list"]').click();
    // Grid should be gone, tree nodes should be back
    await expect(page.locator('[data-testid="grid-view"]')).not.toBeVisible();
    await expect(page.locator('[data-testid^="explorer-node-"]').first()).toBeVisible();
  });

  test("view mode persists across view switches", async ({ page }) => {
    await page.locator('[data-testid="create-page-btn"]').click();

    // Switch to kanban
    await page.locator('[data-testid="view-mode-kanban"]').click();
    await expect(page.locator('[data-testid="kanban-view"]')).toBeVisible();

    // Switch to graph lens and back
    await page.locator('[data-testid="activity-icon-graph"]').click();
    await expect(page.locator(".react-flow")).toBeVisible({ timeout: 10_000 });

    await page.locator('[data-testid="activity-icon-editor"]').click();
    // Kanban should still be active
    await expect(page.locator('[data-testid="kanban-view"]')).toBeVisible();
  });

  test("objects created appear in all views", async ({ page }) => {
    // Create a page
    await page.locator('[data-testid="create-page-btn"]').click();

    // Check list view
    const nodes = page.locator('[data-testid^="explorer-node-"]');
    expect(await nodes.count()).toBeGreaterThan(0);

    // Check kanban view
    await page.locator('[data-testid="view-mode-kanban"]').click();
    expect(await page.locator('[data-testid^="kanban-card-"]').count()).toBeGreaterThan(0);

    // Check grid view
    await page.locator('[data-testid="view-mode-grid"]').click();
    expect(await page.locator('[data-testid^="grid-card-"]').count()).toBeGreaterThan(0);

    // Check table view
    await page.locator('[data-testid="view-mode-table"]').click();
    expect(await page.locator('[data-testid^="table-row-"]').count()).toBeGreaterThan(0);
  });

  test("search works regardless of view mode", async ({ page }) => {
    await page.locator('[data-testid="create-page-btn"]').click();

    // Switch to grid view
    await page.locator('[data-testid="view-mode-grid"]').click();
    await expect(page.locator('[data-testid="grid-view"]')).toBeVisible();

    // Search — should show search results, not grid
    const searchInput = page.locator('[data-testid="explorer-search"]');
    await searchInput.fill("Page");

    await expect(page.locator('[data-testid="search-results"]')).toBeVisible();
    await expect(page.locator('[data-testid="grid-view"]')).not.toBeVisible();

    // Clear search — grid should return
    await searchInput.fill("");
    await expect(page.locator('[data-testid="grid-view"]')).toBeVisible();
  });
});
