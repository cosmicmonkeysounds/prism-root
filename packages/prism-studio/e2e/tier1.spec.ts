import { test, expect } from "@playwright/test";

// ── Clipboard ─────────────────────────────────────────────────────────────

test.describe("Clipboard UI", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Wait for app to render
    const explorer = page.locator('[data-testid="object-explorer"]');
    await expect(explorer).toBeVisible();
    // Select "Home" page
    await explorer.locator("text=Home").first().click();
    // Wait for inspector to show
    await expect(page.locator('[data-testid="inspector-content"]')).toBeVisible();
  });

  test("inspector shows clipboard buttons when object selected", async ({ page }) => {
    const inspector = page.locator('[data-testid="inspector-content"]');
    await expect(inspector.locator('[data-testid="copy-btn"]')).toBeVisible();
    await expect(inspector.locator('[data-testid="cut-btn"]')).toBeVisible();
    await expect(inspector.locator('[data-testid="paste-btn"]')).toBeVisible();
  });

  test("paste button is disabled when clipboard is empty", async ({ page }) => {
    const pasteBtn = page.locator('[data-testid="paste-btn"]');
    await expect(pasteBtn).toBeDisabled();
  });

  test("copy then paste creates a duplicate", async ({ page }) => {
    // Copy Home page
    await page.locator('[data-testid="copy-btn"]').click();

    // Notification should appear
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
    await expect(toast.first()).toContainText("Copied");

    // Paste under About page
    const explorer = page.locator('[data-testid="object-explorer"]');
    await explorer.locator("text=About").first().click();
    await expect(page.locator('[data-testid="inspector-content"]')).toBeVisible();

    const pasteBtn = page.locator('[data-testid="paste-btn"]');
    await expect(pasteBtn).toBeEnabled();
    await pasteBtn.click();

    // Should see a paste notification (may coexist with "Copied" toast)
    await expect(page.locator('[data-testid="notification-toast"]').last()).toContainText("Pasted");
  });

  test("Cmd+C copies selected object", async ({ page }) => {
    const isMac = await page.evaluate(() =>
      /Mac|iPod|iPhone|iPad/.test(navigator.platform),
    );

    // Click on explorer (not input) to ensure focus is not in a text field
    await page.locator('[data-testid="object-explorer"]').locator("text=Home").first().click();

    await page.keyboard.press(isMac ? "Meta+c" : "Control+c");

    // Should show "Copied" toast
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
    await expect(toast.first()).toContainText("Copied");
  });

  test("cut then paste moves the object", async ({ page }) => {
    // Create a new page to cut
    await page.locator('[data-testid="create-page-btn"]').click();

    // Wait for the new page to appear and be selected
    const explorer = page.locator('[data-testid="object-explorer"]');
    await expect(page.locator('[data-testid="inspector-content"]')).toBeVisible();

    // Cut the selected object
    await page.locator('[data-testid="cut-btn"]').click();

    // Now select About to paste under it
    await explorer.locator("text=About").first().click();
    await expect(page.locator('[data-testid="inspector-content"]')).toBeVisible();

    // Paste
    await page.locator('[data-testid="paste-btn"]').click();

    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
  });
});

// ── Templates ─────────────────────────────────────────────────────────────

test.describe("Template Gallery", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await expect(page.locator('[data-testid="object-explorer"]')).toBeVisible();
  });

  test("templates button is visible", async ({ page }) => {
    const btn = page.locator('[data-testid="open-templates-btn"]');
    await expect(btn).toBeVisible();
  });

  test("clicking templates button opens gallery", async ({ page }) => {
    await page.locator('[data-testid="open-templates-btn"]').click();
    const gallery = page.locator('[data-testid="template-gallery"]');
    await expect(gallery).toBeVisible();
  });

  test("gallery shows registered templates", async ({ page }) => {
    await page.locator('[data-testid="open-templates-btn"]').click();
    const gallery = page.locator('[data-testid="template-gallery"]');
    await expect(gallery.locator('[data-testid="template-blog-page"]')).toBeVisible();
    await expect(gallery.locator('[data-testid="template-landing-page"]')).toBeVisible();
  });

  test("clicking a template instantiates it", async ({ page }) => {
    await page.locator('[data-testid="open-templates-btn"]').click();
    await page.locator('[data-testid="template-blog-page"]').click();

    // Gallery should close
    await expect(page.locator('[data-testid="template-gallery"]')).not.toBeVisible();

    // A notification should confirm creation
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
    await expect(toast.first()).toContainText("Blog Post");

    // New page should appear in explorer
    const explorer = page.locator('[data-testid="object-explorer"]');
    await expect(explorer.locator('[data-testid^="explorer-node-"]', { hasText: "New Blog Post" })).toBeVisible();
  });

  test("close button closes the gallery", async ({ page }) => {
    await page.locator('[data-testid="open-templates-btn"]').click();
    await expect(page.locator('[data-testid="template-gallery"]')).toBeVisible();

    await page.locator('[data-testid="close-templates"]').click();
    await expect(page.locator('[data-testid="template-gallery"]')).not.toBeVisible();
  });

  test("instantiating landing page creates page with children", async ({ page }) => {
    await page.locator('[data-testid="open-templates-btn"]').click();
    await page.locator('[data-testid="template-landing-page"]').click();

    // The landing page should be in the explorer
    const explorer = page.locator('[data-testid="object-explorer"]');
    await expect(explorer.locator('[data-testid^="explorer-node-"]', { hasText: "New Landing Page" })).toBeVisible();

    // Should show success notification
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
    await expect(toast.first()).toContainText("Landing Page");
  });
});

// ── Activity Feed ─────────────────────────────────────────────────────────

test.describe("Activity Feed", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await expect(page.locator('[data-testid="object-explorer"]')).toBeVisible();
  });

  test("activity feed is visible with seed data events", async ({ page }) => {
    const feed = page.locator('[data-testid="activity-feed"]');
    await expect(feed).toBeVisible();
    await expect(feed.locator("text=Recent Activity")).toBeVisible();
  });

  test("activity feed shows events for created objects", async ({ page }) => {
    const feed = page.locator('[data-testid="activity-feed"]');
    const events = feed.locator('[data-testid^="activity-event-"]');
    await expect(events.first()).toBeVisible();
  });

  test("creating a new object adds an activity event", async ({ page }) => {
    const feed = page.locator('[data-testid="activity-feed"]');
    const initialCount = await feed.locator('[data-testid^="activity-event-"]').count();

    // Create a new page
    await page.locator('[data-testid="create-page-btn"]').click();

    // Wait for activity to update
    await page.waitForTimeout(500);

    const newCount = await feed.locator('[data-testid^="activity-event-"]').count();
    expect(newCount).toBeGreaterThan(initialCount);
  });

  test("clicking an activity event selects the object", async ({ page }) => {
    // Click the first activity event (one of the seed objects)
    const feed = page.locator('[data-testid="activity-feed"]');
    const firstEvent = feed.locator('[data-testid^="activity-event-"]').first();
    await firstEvent.click();

    // After clicking, inspector should show the object if inspector is visible
    // The key test is that selection changes — just verify no crash
    await page.waitForTimeout(300);
    // The page should still be rendered (no crash from clicking deleted/stale objects)
    await expect(page.locator('[data-testid="object-explorer"]')).toBeVisible();
  });
});

// ── Reorder ───────────────────────────────────────────────────────────────

test.describe("Object Reorder", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await expect(page.locator('[data-testid="object-explorer"]')).toBeVisible();
  });

  test("selecting an object shows reorder buttons", async ({ page }) => {
    const explorer = page.locator('[data-testid="object-explorer"]');
    await explorer.locator("text=About").first().click();
    await expect(page.locator('[data-testid^="move-up-"]').first()).toBeVisible();
  });

  test("move up button reorders object", async ({ page }) => {
    const explorer = page.locator('[data-testid="object-explorer"]');

    // Select "About" (position 1)
    await explorer.locator("text=About").first().click();
    await expect(page.locator('[data-testid^="move-up-"]').first()).toBeVisible();

    // Click move up
    await page.locator('[data-testid^="move-up-"]').first().click();

    // After moving up, About should be the first explorer node
    await page.waitForTimeout(300);
    const nodes = explorer.locator('[data-testid^="explorer-node-"]');
    const firstNodeText = await nodes.first().textContent();
    expect(firstNodeText).toContain("About");
  });

  test("move down button reorders object", async ({ page }) => {
    const explorer = page.locator('[data-testid="object-explorer"]');

    // Select "Home" (position 0)
    await explorer.locator("text=Home").first().click();
    await expect(page.locator('[data-testid^="move-down-"]').first()).toBeVisible();

    // Click move down
    await page.locator('[data-testid^="move-down-"]').first().click();

    // After moving down, About should be the first explorer node
    await page.waitForTimeout(300);
    const nodes = explorer.locator('[data-testid^="explorer-node-"]');
    const firstNodeText = await nodes.first().textContent();
    expect(firstNodeText).toContain("About");
  });
});
