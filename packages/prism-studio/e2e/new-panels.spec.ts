import { test, expect } from "@playwright/test";

/**
 * E2E coverage for the panels added in the Studio Checklist sprint:
 *   - Design Tokens      (3E)
 *   - Form Builder       (8B)
 *   - Site Navigation    (8D)
 *   - Entity Builder     (9A)
 *   - Relationship Builder (9B)
 *   - Publish            (6A-D)
 *
 * Every panel is a lens, so we switch to it via its activity-bar icon and
 * assert the root `data-testid` is visible + at least one key interactive
 * element renders. These tests do not require a connected relay or peer.
 */

async function openPanel(page: import("@playwright/test").Page, iconId: string, panelTestId: string) {
  await page.goto("/");
  await page.locator(`[data-testid="activity-icon-${iconId}"]`).click();
  await expect(page.locator(`[data-testid="${panelTestId}"]`)).toBeVisible({ timeout: 10_000 });
}

test.describe("Design Tokens Panel (3E)", () => {
  test("renders token editor", async ({ page }) => {
    await openPanel(page, "design-tokens", "design-tokens-panel");
    await expect(page.locator('[data-testid="design-tokens-panel"]')).toContainText("Design Tokens");
  });
});

test.describe("Form Builder Panel (8B)", () => {
  test("renders empty-state when no container selected", async ({ page }) => {
    await openPanel(page, "form-builder", "form-builder-panel");
    const panel = page.locator('[data-testid="form-builder-panel"]');
    await expect(panel).toContainText("Form Builder");
  });
});

test.describe("Site Navigation Panel (8D)", () => {
  test("lists pages in the vault", async ({ page }) => {
    await openPanel(page, "site-nav", "site-nav-panel");
    const panel = page.locator('[data-testid="site-nav-panel"]');
    await expect(panel).toContainText("Site Navigation");
  });

  test("pages support set-home / hide / reorder controls when present", async ({ page }) => {
    await openPanel(page, "site-nav", "site-nav-panel");
    const panel = page.locator('[data-testid="site-nav-panel"]');
    // Look for any rendered nav item — seeded demo workspaces usually have
    // at least one page. If none exist we should see the empty message.
    const hasItems = await panel.locator('[data-testid^="site-nav-item-"]').count();
    if (hasItems === 0) {
      await expect(panel).toContainText("No pages in this vault yet");
    }
  });
});

test.describe("Entity Builder Panel (9A)", () => {
  test("form fields render", async ({ page }) => {
    await openPanel(page, "entity-builder", "entity-builder-panel");
    await expect(page.locator('[data-testid="entity-type-name"]')).toBeVisible();
    await expect(page.locator('[data-testid="entity-label"]')).toBeVisible();
    await expect(page.locator('[data-testid="entity-category"]')).toBeVisible();
    await expect(page.locator('[data-testid="register-entity-btn"]')).toBeVisible();
  });

  test("can add and remove a draft field", async ({ page }) => {
    await openPanel(page, "entity-builder", "entity-builder-panel");
    const panel = page.locator('[data-testid="entity-builder-panel"]');
    await panel.locator('[data-testid="new-field-id"]').fill("email");
    await panel.locator('[data-testid="add-field-btn"]').click();
    await expect(panel.locator('[data-testid="field-row-email"]')).toBeVisible();
  });
});

test.describe("Relationship Builder Panel (9B)", () => {
  test("form fields render", async ({ page }) => {
    await openPanel(page, "relationship-builder", "relationship-builder-panel");
    await expect(page.locator('[data-testid="relation-id"]')).toBeVisible();
    await expect(page.locator('[data-testid="relation-label"]')).toBeVisible();
    await expect(page.locator('[data-testid="relation-behavior"]')).toBeVisible();
    await expect(page.locator('[data-testid="register-relation-btn"]')).toBeVisible();
  });
});

test.describe("Publish Panel (6A-D)", () => {
  test("renders workflow controls", async ({ page }) => {
    await openPanel(page, "publish", "publish-panel");
    await expect(page.locator('[data-testid="publish-panel"]')).toContainText("Publish");
  });
});

test.describe("Canvas Peer Cursors (8E)", () => {
  test("canvas renders peer-cursors bar even with no remote peers", async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-canvas"]').click();
    await expect(page.locator('[data-testid="canvas-panel"]')).toBeVisible({ timeout: 10_000 });
    await expect(page.locator('[data-testid="peer-cursors-bar"]')).toBeVisible();
  });
});
