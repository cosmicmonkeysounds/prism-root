import { test, expect } from "@playwright/test";

test.describe("Prism Studio", () => {
  test("should render the workspace shell with activity bar", async ({ page }) => {
    await page.goto("/");
    const shell = page.locator('[data-testid="workspace-shell"]');
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

    // Click CRDT lens
    await page.locator('[data-testid="activity-icon-crdt"]').click();
    await expect(page.locator("text=CRDT State Inspector")).toBeVisible();

    // Click Graph lens
    await page.locator('[data-testid="activity-icon-graph"]').click();
    await expect(page.locator(".react-flow")).toBeVisible({ timeout: 10_000 });
  });

  test("should write a value to CRDT via the inspector", async ({ page }) => {
    await page.goto("/");

    // Open CRDT lens
    await page.locator('[data-testid="activity-icon-crdt"]').click();
    await expect(page.locator("text=CRDT State Inspector")).toBeVisible();

    // Fill in key and value
    const keyInput = page.locator("input").first();
    const valueInput = page.locator("input").nth(1);

    await keyInput.fill("test-key");
    await valueInput.fill("test-value");

    // Click Set button
    await page.getByRole("button", { name: "Set" }).click();

    // Verify the value appears in the state display
    const stateDisplay = page.locator("pre");
    await expect(stateDisplay).toContainText("test-key");
    await expect(stateDisplay).toContainText("test-value");
  });

  test("should write multiple values and display all", async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-crdt"]').click();

    const keyInput = page.locator("input").first();
    const valueInput = page.locator("input").nth(1);

    await keyInput.fill("name");
    await valueInput.fill("Prism");
    await page.getByRole("button", { name: "Set" }).click();

    await keyInput.fill("version");
    await valueInput.fill("0.1.0");
    await page.getByRole("button", { name: "Set" }).click();

    const stateDisplay = page.locator("pre");
    await expect(stateDisplay).toContainText("Prism");
    await expect(stateDisplay).toContainText("0.1.0");
  });

  test("should overwrite existing key with new value", async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-crdt"]').click();

    const keyInput = page.locator("input").first();
    const valueInput = page.locator("input").nth(1);

    await keyInput.fill("status");
    await valueInput.fill("draft");
    await page.getByRole("button", { name: "Set" }).click();

    await keyInput.fill("status");
    await valueInput.fill("published");
    await page.getByRole("button", { name: "Set" }).click();

    const stateDisplay = page.locator("pre");
    await expect(stateDisplay).toContainText("published");
  });
});
