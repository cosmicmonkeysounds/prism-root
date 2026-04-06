import { test, expect } from "@playwright/test";

test.describe("Phase 3: The Graph", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Navigate to Graph lens via activity bar
    await page.locator('[data-testid="activity-icon-graph"]').click();
  });

  test("should render the Graph tab with ReactFlow canvas", async ({
    page,
  }) => {
    const flow = page.locator(".react-flow");
    await expect(flow).toBeVisible({ timeout: 10_000 });
  });

  test("should display seed nodes in the graph", async ({ page }) => {
    const flow = page.locator(".react-flow");
    await expect(flow).toBeVisible({ timeout: 10_000 });

    // Seed data: Home, Hero, Content, Main Heading, Intro Text, About = 6 nodes
    const nodes = page.locator(".react-flow__node");
    await expect(nodes).toHaveCount(6, { timeout: 5_000 });
  });

  test("should display seed edges in the graph", async ({ page }) => {
    const flow = page.locator(".react-flow");
    await expect(flow).toBeVisible({ timeout: 10_000 });

    // Parent-child edges: Home->Hero, Home->Content, Content->Main Heading, Content->Intro Text = 4
    const edges = page.locator(".react-flow__edge");
    await expect(edges).toHaveCount(4, { timeout: 5_000 });
  });

  test("should render node content", async ({ page }) => {
    const flow = page.locator(".react-flow");
    await expect(flow).toBeVisible({ timeout: 10_000 });

    // Nodes have labels with object names
    const nodes = page.locator(".react-flow__node");
    await expect(nodes.first()).toBeVisible({ timeout: 5_000 });
  });

  test("should navigate to Graph tab via KBar", async ({ page }) => {
    // Start on editor tab
    await page.locator('[data-testid="activity-icon-editor"]').click();

    const isMac = await page.evaluate(() =>
      /Mac|iPod|iPhone|iPad/.test(navigator.platform),
    );
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");

    const kbarInput = page.locator('[role="combobox"]');
    await expect(kbarInput).toBeVisible({ timeout: 3_000 });
    await kbarInput.type("Graph", { delay: 50 });

    const result = page.locator("text=Switch to Graph");
    await expect(result).toBeVisible({ timeout: 3_000 });
    await result.click();

    const flow = page.locator(".react-flow");
    await expect(flow).toBeVisible({ timeout: 10_000 });
  });
});
