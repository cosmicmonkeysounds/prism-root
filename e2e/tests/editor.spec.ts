import { test, expect } from "@playwright/test";

test.describe("Editor Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Editor is the default lens — wait for CodeMirror to mount
    await expect(page.locator("text=editor_content (LoroText)")).toBeVisible({
      timeout: 5_000,
    });
  });

  test("should render CodeMirror editor with default content", async ({
    page,
  }) => {
    const cm = page.locator(".cm-editor");
    await expect(cm).toBeVisible({ timeout: 5_000 });
    // Default content includes Lua greeting
    await expect(cm).toContainText("Hello from Prism");
  });

  test("should allow typing in the editor", async ({ page }) => {
    const cm = page.locator(".cm-editor");
    await expect(cm).toBeVisible({ timeout: 5_000 });

    // Click into the editor content area
    const content = page.locator(".cm-content");
    await content.click();

    // Type some text at cursor position
    await page.keyboard.type("-- test line\n");

    // Verify the typed text appears
    await expect(cm).toContainText("test line");
  });

  test("should show line numbers", async ({ page }) => {
    const cm = page.locator(".cm-editor");
    await expect(cm).toBeVisible({ timeout: 5_000 });
    // Should have gutter elements (line numbers or other gutter markers)
    const gutterElements = page.locator(".cm-gutterElement");
    const count = await gutterElements.count();
    expect(count).toBeGreaterThan(0);
  });

  test("should persist edits across lens switches", async ({ page }) => {
    const cm = page.locator(".cm-editor");
    await expect(cm).toBeVisible({ timeout: 5_000 });

    // Type unique text
    const content = page.locator(".cm-content");
    await content.click();
    await page.keyboard.type("UNIQUE_MARKER_TEXT");

    // Switch to CRDT lens
    await page.locator('[data-testid="activity-icon-crdt"]').click();
    await expect(page.locator('[data-testid="tab-crdt"]')).toBeVisible();

    // Switch back to editor
    await page.locator('[data-testid="activity-icon-editor"]').click();
    await expect(page.locator("text=editor_content (LoroText)")).toBeVisible({
      timeout: 5_000,
    });

    // The typed text should still be there (Loro CRDT is source of truth)
    const editor = page.locator(".cm-editor");
    await expect(editor).toContainText("UNIQUE_MARKER_TEXT");
  });
});
