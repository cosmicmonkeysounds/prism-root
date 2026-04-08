import { test, expect } from "@playwright/test";

/**
 * Schema Designer panel — E2E tests.
 *
 * Covers the visual-canvas write path for the entity/edge type registry:
 *  - Panel mounts and xyflow canvas is wired
 *  - Entity create/delete via toolbar (window.prompt is stubbed)
 *  - Node list reflects registry mutations
 *
 * Lens shortcut: Shift+D. Activity-bar testid: activity-icon-schema-designer.
 */

test.describe("Schema Designer Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page
      .locator('[data-testid="activity-icon-schema-designer"]')
      .click();
    await expect(
      page.locator('[data-testid="schema-designer-panel"]'),
    ).toBeVisible({ timeout: 10_000 });
  });

  test("panel mounts with toolbar and canvas", async ({ page }) => {
    const panel = page.locator('[data-testid="schema-designer-panel"]');
    await expect(panel.locator("text=Schema Designer")).toBeVisible();
    await expect(
      panel.locator('[data-testid="schema-add-entity-btn"]'),
    ).toBeVisible();
    await expect(
      panel.locator('[data-testid="schema-delete-btn"]'),
    ).toBeVisible();
  });

  test("shows existing entity nodes from the registry", async ({ page }) => {
    const panel = page.locator('[data-testid="schema-designer-panel"]');
    // Registry is seeded with the page-builder entity types (page, section, etc.)
    // so at least one entity-node must render on mount.
    const nodes = panel.locator('[data-testid="schema-entity-node"]');
    await expect(nodes.first()).toBeVisible({ timeout: 10_000 });
    const count = await nodes.count();
    expect(count).toBeGreaterThan(0);
  });

  test("toolbar counts update when a new entity is added", async ({ page }) => {
    const panel = page.locator('[data-testid="schema-designer-panel"]');

    // Count the "N entity/types" label before add.
    const countRegex = /(\d+)\s+entit/;
    const beforeText = (await panel.textContent()) ?? "";
    const beforeMatch = beforeText.match(countRegex);
    const before = beforeMatch ? Number(beforeMatch[1]) : 0;

    // Stub window.prompt so the panel's addEntity() flow completes.
    await page.evaluate(() => {
      (window as unknown as { prompt: (m: string) => string | null }).prompt =
        () => "e2e-entity-test";
    });

    await panel.locator('[data-testid="schema-add-entity-btn"]').click();

    // After add, the count should have incremented.
    await expect
      .poll(async () => {
        const t = (await panel.textContent()) ?? "";
        const m = t.match(countRegex);
        return m ? Number(m[1]) : 0;
      })
      .toBe(before + 1);
  });

  test("delete button is disabled when nothing is selected", async ({ page }) => {
    const panel = page.locator('[data-testid="schema-designer-panel"]');
    await expect(
      panel.locator('[data-testid="schema-delete-btn"]'),
    ).toBeDisabled();
  });

  test("lens appears in the activity bar", async ({ page }) => {
    await expect(
      page.locator('[data-testid="activity-icon-schema-designer"]'),
    ).toBeVisible();
  });
});
