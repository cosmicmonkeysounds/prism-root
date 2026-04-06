import { test, expect } from "@playwright/test";

test.describe("Prism Studio", () => {
  test("should render the lens shell with activity bar", async ({ page }) => {
    await page.goto("/");
    const shell = page.locator('[data-testid="lens-shell"]');
    await expect(shell).toBeVisible();
    const activityBar = page.locator('[data-testid="activity-bar"]');
    await expect(activityBar).toBeVisible();
  });

  test("should show editor tab open by default", async ({ page }) => {
    await page.goto("/");
    const tab = page.locator('[data-testid="tab-editor"]');
    await expect(tab).toBeVisible();
  });

  test("should switch between lenses via activity bar", async ({ page }) => {
    await page.goto("/");

    // Click CRDT lens — should show Collection Inspector
    await page.locator('[data-testid="activity-icon-crdt"]').click();
    await expect(page.locator("text=Collection Inspector")).toBeVisible({ timeout: 5000 });

    // Click Graph lens
    await page.locator('[data-testid="activity-icon-graph"]').click();
    await expect(page.locator(".react-flow")).toBeVisible({ timeout: 10_000 });
  });

  // The CRDT panel is now a Collection Inspector with objects/edges/json tabs,
  // not a key-value form. Skip the old key-value tests.
  test.skip("should write a value to CRDT via the inspector", async () => {});
  test.skip("should write multiple values and display all", async () => {});
  test.skip("should overwrite existing key with new value", async () => {});
});
