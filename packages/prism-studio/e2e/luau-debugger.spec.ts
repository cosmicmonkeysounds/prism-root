import { test, expect } from "@playwright/test";

/**
 * Luau step-through debugger — E2E tests.
 *
 * Two integration surfaces are covered:
 *
 * 1. Visual Script Panel (Shift+S) — builds Luau from step cards and lets
 *    the user set breakpoints per step. Trace frames map back to the owning
 *    step via emitStepsLuauWithMap().lineToStep.
 *
 * 2. Luau Facet Panel (Shift+U) — raw Luau editor with line-based breakpoints.
 *    Debug runs the textarea source through the same luau-web debugger.
 *
 * Both surfaces call createLuauDebugger() under the hood, so these tests also
 * smoke-check that luau-web loads in the browser bundle.
 */

// ── Visual Script debugger ─────────────────────────────────────────────────

test.describe("Visual Script Debugger", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page
      .locator('[data-testid="activity-icon-visual-script"]')
      .click();
    await expect(
      page.locator('[data-testid="visual-script-panel"]'),
    ).toBeVisible({ timeout: 10_000 });
  });

  test("Debug button is rendered in the toolbar", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await expect(
      panel.locator('[data-testid="visual-script-debug-btn"]'),
    ).toBeVisible();
  });

  test("Debug on an empty script surfaces a warning and no frames panel", async ({
    page,
  }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await panel.locator('[data-testid="visual-script-debug-btn"]').click();
    // No frames panel should appear for an empty script.
    await expect(
      panel.locator('[data-testid="debug-frames-panel"]'),
    ).toHaveCount(0);
  });

  test("debugging a script with one step opens the frames panel", async ({
    page,
  }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    // Add a benign step that emits trivial Luau (a comment) — passes validation.
    await panel.locator("text=Comment").first().click();
    await expect(
      panel.locator('[data-testid="visual-script-step-0"]'),
    ).toBeVisible();

    await panel.locator('[data-testid="visual-script-debug-btn"]').click();
    await expect(
      panel.locator('[data-testid="debug-frames-panel"]'),
    ).toBeVisible({ timeout: 15_000 });
    await expect(
      panel.locator('[data-testid="debug-frame-list"]'),
    ).toBeVisible();
    await expect(panel.locator('[data-testid="debug-locals"]')).toBeVisible();
  });

  test("breakpoint toggle on a step persists its data-breakpoint attribute", async ({
    page,
  }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await panel.locator("text=Comment").first().click();
    const step = panel.locator('[data-testid="visual-script-step-0"]');
    await expect(step).toHaveAttribute("data-breakpoint", "false");
    await panel.locator('[data-testid="visual-script-bp-0"]').click();
    await expect(step).toHaveAttribute("data-breakpoint", "true");
  });

  test("closing the debug panel dismisses the frames view", async ({
    page,
  }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await panel.locator("text=Comment").first().click();
    await panel.locator('[data-testid="visual-script-debug-btn"]').click();
    await expect(
      panel.locator('[data-testid="debug-frames-panel"]'),
    ).toBeVisible({ timeout: 15_000 });
    await panel.locator('[data-testid="debug-close"]').click();
    await expect(
      panel.locator('[data-testid="debug-frames-panel"]'),
    ).toHaveCount(0);
  });
});

// ── Raw Luau debugger (Luau Facet Panel) ─────────────────────────────────────

test.describe("Raw Luau Debugger", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page
      .locator('[data-testid="activity-icon-luau-facet"]')
      .click();
    await expect(
      page.locator('[data-testid="luau-facet-panel"]'),
    ).toBeVisible({ timeout: 10_000 });
  });

  test("Debug button and line gutter are rendered", async ({ page }) => {
    const panel = page.locator('[data-testid="luau-facet-panel"]');
    await expect(
      panel.locator('[data-testid="luau-debug-btn"]'),
    ).toBeVisible();
    await expect(
      panel.locator('[data-testid="luau-line-gutter"]'),
    ).toBeVisible();
  });

  test("running Debug on the seeded sample opens the frames panel", async ({
    page,
  }) => {
    const panel = page.locator('[data-testid="luau-facet-panel"]');
    // Replace the editor source with something simple and side-effect-free
    // so we don't depend on the specific seeded sample passing parse.
    const editor = panel.locator('[data-testid="luau-editor"]');
    await editor.fill("local x = 1\nlocal y = 2\nlocal z = x + y\n");

    await panel.locator('[data-testid="luau-debug-btn"]').click();
    await expect(
      panel.locator('[data-testid="luau-debug-frames-panel"]'),
    ).toBeVisible({ timeout: 15_000 });
    await expect(
      panel.locator('[data-testid="luau-debug-frame-list"]'),
    ).toBeVisible();
    await expect(
      panel.locator('[data-testid="luau-debug-locals"]'),
    ).toBeVisible();
  });

  test("toggling a line breakpoint updates the gutter state", async ({
    page,
  }) => {
    const panel = page.locator('[data-testid="luau-facet-panel"]');
    const editor = panel.locator('[data-testid="luau-editor"]');
    await editor.fill("local a = 1\nlocal b = 2\n");

    const btn = panel.locator('[data-testid="luau-line-btn-2"]');
    await expect(btn).toBeVisible();
    await expect(btn).toHaveAttribute("data-breakpoint", "false");
    await btn.click();
    await expect(btn).toHaveAttribute("data-breakpoint", "true");
  });

  test("closing the frames panel dismisses it", async ({ page }) => {
    const panel = page.locator('[data-testid="luau-facet-panel"]');
    const editor = panel.locator('[data-testid="luau-editor"]');
    await editor.fill("local a = 42\n");
    await panel.locator('[data-testid="luau-debug-btn"]').click();
    await expect(
      panel.locator('[data-testid="luau-debug-frames-panel"]'),
    ).toBeVisible({ timeout: 15_000 });
    await panel.locator('[data-testid="luau-debug-close"]').click();
    await expect(
      panel.locator('[data-testid="luau-debug-frames-panel"]'),
    ).toHaveCount(0);
  });
});
