import { test, expect } from "@playwright/test";

test.describe("Phase 3: The Graph", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Navigate to Graph tab
    await page.getByRole("button", { name: "Graph" }).click();
  });

  test("should render the Graph tab with ReactFlow canvas", async ({
    page,
  }) => {
    // ReactFlow renders a .react-flow container
    const flow = page.locator(".react-flow");
    await expect(flow).toBeVisible({ timeout: 10_000 });
  });

  test("should display seed nodes in the graph", async ({ page }) => {
    // Wait for ReactFlow to render nodes
    const flow = page.locator(".react-flow");
    await expect(flow).toBeVisible({ timeout: 10_000 });

    // The graph panel seeds 3 demo nodes
    const nodes = page.locator(".react-flow__node");
    await expect(nodes).toHaveCount(3, { timeout: 5_000 });
  });

  test("should display seed edges in the graph", async ({ page }) => {
    const flow = page.locator(".react-flow");
    await expect(flow).toBeVisible({ timeout: 10_000 });

    // 2 edges: one hardRef, one weakRef
    const edges = page.locator(".react-flow__edge");
    await expect(edges).toHaveCount(2, { timeout: 5_000 });
  });

  test("should render custom node content", async ({ page }) => {
    const flow = page.locator(".react-flow");
    await expect(flow).toBeVisible({ timeout: 10_000 });

    // CodeMirror node should show "main.ts" header
    await expect(page.locator(".prism-node-header").first()).toBeVisible({
      timeout: 5_000,
    });
  });

  test("should navigate to Graph tab via KBar", async ({ page }) => {
    // Start on editor tab
    await page.getByRole("button", { name: "Editor" }).click();

    // KBar uses tinykeys $mod+k — resolves to Control on headless Chrome
    // (navigator.platform is not Mac in headless shell)
    const isMac = await page.evaluate(() =>
      /Mac|iPod|iPhone|iPad/.test(navigator.platform),
    );
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");

    // KBar search input
    const kbarInput = page.locator('[role="combobox"]');
    await expect(kbarInput).toBeVisible({ timeout: 3_000 });
    await kbarInput.type("Graph", { delay: 50 });

    // Wait for results to appear then select
    const result = page.locator("text=Switch to Graph");
    await expect(result).toBeVisible({ timeout: 3_000 });
    await result.click();

    // Should now be on graph tab
    const flow = page.locator(".react-flow");
    await expect(flow).toBeVisible({ timeout: 10_000 });
  });
});
