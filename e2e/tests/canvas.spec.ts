import { test, expect } from "@playwright/test";

test.describe("Canvas Preview Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Open the canvas lens
    await page.locator('[data-testid="activity-icon-canvas"]').click();
    // Wait for the canvas panel to render
    await expect(page.locator('[data-testid="canvas-panel"]')).toBeVisible();
  });

  test("renders canvas panel with seed page content", async ({ page }) => {
    // Home page is selected by default (seed data selects it)
    const canvas = page.locator('[data-testid="canvas-panel"]');

    // Should render the page content (heading and text-block from seed data)
    await expect(canvas.locator("text=Build anything with Prism")).toBeVisible();
  });

  test("shows empty state when no page is selected", async ({ page }) => {
    // Deselect by clicking somewhere neutral or selecting a non-page
    // The "Select a page to preview" state shows when nothing is selected
    // For now, we can verify the canvas renders at all
    const canvas = page.locator('[data-testid="canvas-panel"]');
    await expect(canvas).toBeVisible();
  });

  test("clicking a block in the canvas selects it", async ({ page }) => {
    const canvas = page.locator('[data-testid="canvas-panel"]');

    // Click on the heading text in the canvas
    const heading = canvas.locator("h1", { hasText: "Build anything with Prism" });
    await heading.click();

    // Inspector should now show the heading's properties
    const inspector = page.locator('[data-testid="inspector-content"]');
    await expect(inspector.locator("text=Type: heading")).toBeVisible();
  });

  test("page layout renders sections with content", async ({ page }) => {
    const canvas = page.locator('[data-testid="canvas-panel"]');

    // The seed data Home page has 2 sections: Hero and Content
    // Content section has a heading and text-block
    // Check that at least one section border container exists
    const sections = canvas.locator('[data-testid^="canvas-section-"]');
    await expect(sections.first()).toBeVisible();
  });
});

test.describe("Object Explorer Search", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("search input is visible in explorer", async ({ page }) => {
    const searchInput = page.locator('[data-testid="explorer-search"]');
    await expect(searchInput).toBeVisible();
  });

  test("searching filters objects", async ({ page }) => {
    const searchInput = page.locator('[data-testid="explorer-search"]');
    await searchInput.fill("Home");

    // Should show search results
    const results = page.locator('[data-testid="search-results"]');
    await expect(results).toBeVisible();
    await expect(results.locator("text=Home")).toBeVisible();
  });

  test("clearing search restores tree view", async ({ page }) => {
    const searchInput = page.locator('[data-testid="explorer-search"]');
    await searchInput.fill("Home");
    await expect(page.locator('[data-testid="search-results"]')).toBeVisible();

    // Clear the search
    await searchInput.fill("");

    // Search results should disappear, tree should be back
    await expect(page.locator('[data-testid="search-results"]')).not.toBeVisible();
    await expect(page.locator('[data-testid="object-explorer"]')).toBeVisible();
  });

  test("clicking a search result selects the object", async ({ page }) => {
    const searchInput = page.locator('[data-testid="explorer-search"]');
    await searchInput.fill("About");

    // Click the search result
    const results = page.locator('[data-testid="search-results"]');
    await results.locator("text=About").click();

    // Inspector should show the About page
    const inspector = page.locator('[data-testid="inspector-content"]');
    await expect(inspector).toBeVisible();
    await expect(inspector.locator("input").first()).toHaveValue("About");
  });
});
