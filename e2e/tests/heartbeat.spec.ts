import { test, expect } from "@playwright/test";

test.describe("Prism Studio", () => {
  test("should render the studio header and tabs", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("strong")).toContainText("Prism Studio");
    // All four tabs should be visible
    for (const label of ["Editor", "Layout", "Graph", "CRDT"]) {
      await expect(page.getByRole("button", { name: label })).toBeVisible();
    }
  });

  test("should show editor tab by default", async ({ page }) => {
    await page.goto("/");
    // Editor tab is active (dark background) and the CodeMirror editor is present
    const editorBtn = page.getByRole("button", { name: "Editor" });
    await expect(editorBtn).toHaveCSS("background-color", "rgb(51, 51, 51)");
  });

  test("should switch between tabs", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: "CRDT" }).click();
    await expect(page.locator("text=CRDT State Inspector")).toBeVisible();

    await page.getByRole("button", { name: "Layout" }).click();
    // Layout tab should be showing (Puck builder)
    await expect(page.locator("text=CRDT State Inspector")).not.toBeVisible();

    await page.getByRole("button", { name: "Graph" }).click();
    await expect(page.locator("text=CRDT State Inspector")).not.toBeVisible();
  });

  test("should write a value to CRDT via the inspector", async ({ page }) => {
    await page.goto("/");

    // Navigate to full-width CRDT tab
    await page.getByRole("button", { name: "CRDT" }).click();
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
    await page.getByRole("button", { name: "CRDT" }).click();

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
    await page.getByRole("button", { name: "CRDT" }).click();

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

  test("CRDT sidebar is visible in editor tab", async ({ page }) => {
    await page.goto("/");
    // Editor tab is default — CRDT inspector should be in the sidebar
    await expect(page.locator("text=CRDT State Inspector")).toBeVisible();
  });
});
