import { test, expect } from "@playwright/test";

/**
 * App Builder E2E
 *
 * Exercises the self-replicating Studio feature: composing focused
 * Prism apps (Flux, Lattice, Cadence, Grip) and Relay deployments as
 * build targets. All runs happen in dry-run mode (no daemon required).
 */

test.describe("App Builder Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-app-builder"]').click();
    await expect(page.locator('[data-testid="app-builder-panel"]')).toBeVisible();
  });

  test("renders the six built-in profiles", async ({ page }) => {
    const grid = page.locator('[data-testid="builder-profile-grid"]');
    await expect(grid).toBeVisible();
    await expect(page.locator('[data-testid="builder-profile-studio"]')).toBeVisible();
    await expect(page.locator('[data-testid="builder-profile-flux"]')).toBeVisible();
    await expect(page.locator('[data-testid="builder-profile-lattice"]')).toBeVisible();
    await expect(page.locator('[data-testid="builder-profile-cadence"]')).toBeVisible();
    await expect(page.locator('[data-testid="builder-profile-grip"]')).toBeVisible();
    await expect(page.locator('[data-testid="builder-profile-relay"]')).toBeVisible();
  });

  test("renders every build target pill", async ({ page }) => {
    const targets = page.locator('[data-testid="builder-target-list"]');
    await expect(targets).toBeVisible();
    await expect(page.locator('[data-testid="builder-target-web"]')).toBeVisible();
    await expect(page.locator('[data-testid="builder-target-tauri"]')).toBeVisible();
    await expect(page.locator('[data-testid="builder-target-capacitor-ios"]')).toBeVisible();
    await expect(page.locator('[data-testid="builder-target-capacitor-android"]')).toBeVisible();
    await expect(page.locator('[data-testid="builder-target-relay-node"]')).toBeVisible();
    await expect(page.locator('[data-testid="builder-target-relay-docker"]')).toBeVisible();
  });

  test("selecting the Flux profile updates the details card", async ({ page }) => {
    await page.locator('[data-testid="builder-profile-flux"]').click();
    const details = page.locator('[data-testid="builder-profile-details"]');
    await expect(details).toBeVisible();
    await expect(details.locator("code", { hasText: "flux" })).toBeVisible();
  });

  test("preview plan button emits a BuildPlan for Flux + Web", async ({ page }) => {
    await page.locator('[data-testid="builder-profile-flux"]').click();
    // Web target is selected by default; confirm it's active
    await page.locator('[data-testid="builder-preview-plan"]').click();

    const preview = page.locator('[data-testid="builder-plan-preview"]');
    await expect(preview).toBeVisible();
    await expect(preview.locator("text=Flux").first()).toBeVisible();
    await expect(preview.locator("text=web").first()).toBeVisible();

    // At least one step and one artifact should be rendered
    await expect(page.locator('[data-testid="builder-plan-step-0"]')).toBeVisible();
    await expect(page.locator('[data-testid="builder-plan-artifact-0"]')).toBeVisible();
  });

  test("dry-run build records a successful BuildRun", async ({ page }) => {
    await page.locator('[data-testid="builder-profile-flux"]').click();
    await page.locator('[data-testid="builder-run-build"]').click();

    const lastRun = page.locator('[data-testid="builder-last-run"]');
    await expect(lastRun).toBeVisible();
    // First step is always emit-file (the profile JSON), which succeeds
    await expect(page.locator('[data-testid="builder-run-step-0"]')).toBeVisible();
    await expect(page.locator('[data-testid="builder-run-history"]')).toBeVisible();
  });

  test("selecting multiple targets runs all of them", async ({ page }) => {
    await page.locator('[data-testid="builder-profile-flux"]').click();
    // Add Tauri target
    await page.locator('[data-testid="builder-target-tauri"]').click();
    // Build both
    await page.locator('[data-testid="builder-run-build"]').click();

    // History should show at least two runs (web + tauri)
    const history = page.locator('[data-testid="builder-run-history"]');
    await expect(history).toBeVisible();
    // Both targets should appear somewhere in the history
    await expect(history.locator("text=web").first()).toBeVisible();
    await expect(history.locator("text=tauri").first()).toBeVisible();
  });

  test("Relay profile builds the relay-docker target", async ({ page }) => {
    await page.locator('[data-testid="builder-profile-relay"]').click();
    // Deselect web (it was the default), select relay-docker
    await page.locator('[data-testid="builder-target-web"]').click();
    await page.locator('[data-testid="builder-target-relay-docker"]').click();

    await page.locator('[data-testid="builder-preview-plan"]').click();

    const preview = page.locator('[data-testid="builder-plan-preview"]');
    await expect(preview).toBeVisible();
    await expect(preview.locator("text=relay-docker").first()).toBeVisible();
    // The raw JSON should mention docker
    await expect(preview.locator("text=docker").first()).toBeVisible();
  });

  test("setting an active profile pins it", async ({ page }) => {
    await page.locator('[data-testid="builder-profile-flux"]').click();
    await page.locator('[data-testid="builder-toggle-active-profile"]').click();

    // The Flux card should now show an "active" badge
    const fluxCard = page.locator('[data-testid="builder-profile-flux"]');
    await expect(fluxCard.locator("text=active")).toBeVisible();
  });

  test("clearing active profile removes the badge", async ({ page }) => {
    await page.locator('[data-testid="builder-profile-flux"]').click();
    await page.locator('[data-testid="builder-toggle-active-profile"]').click();
    await page.locator('[data-testid="builder-toggle-active-profile"]').click();

    const fluxCard = page.locator('[data-testid="builder-profile-flux"]');
    await expect(fluxCard.locator("text=active")).not.toBeVisible();
  });

  test("plan preview exposes raw BuildPlan JSON", async ({ page }) => {
    await page.locator('[data-testid="builder-profile-flux"]').click();
    await page.locator('[data-testid="builder-preview-plan"]').click();

    // The JSON is behind a <details>; force it open by clicking
    const preview = page.locator('[data-testid="builder-plan-preview"]');
    await preview.locator("text=Raw JSON").click();
    const json = page.locator('[data-testid="builder-plan-json"]');
    await expect(json).toBeVisible();
    await expect(json).toContainText('"profileId"');
    await expect(json).toContainText('"target"');
  });

  test("clear run history button empties the list", async ({ page }) => {
    await page.locator('[data-testid="builder-profile-flux"]').click();
    await page.locator('[data-testid="builder-run-build"]').click();
    await expect(page.locator('[data-testid="builder-run-history"]')).toBeVisible();

    await page.locator('[data-testid="builder-clear-runs"]').click();
    await expect(page.locator('[data-testid="builder-run-history"]')).not.toBeVisible();
  });
});
