# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: e2e/tests/graph.spec.ts >> Phase 3: The Graph >> should navigate to Graph tab via KBar
- Location: e2e/tests/graph.spec.ts:47:3

# Error details

```
Error: page.goto: Protocol error (Page.navigate): Cannot navigate to invalid URL
Call log:
  - navigating to "/", waiting until "load"

```

# Test source

```ts
  1  | import { test, expect } from "@playwright/test";
  2  | 
  3  | test.describe("Phase 3: The Graph", () => {
  4  |   test.beforeEach(async ({ page }) => {
> 5  |     await page.goto("/");
     |                ^ Error: page.goto: Protocol error (Page.navigate): Cannot navigate to invalid URL
  6  |     // Navigate to Graph tab
  7  |     await page.getByRole("button", { name: "Graph" }).click();
  8  |   });
  9  | 
  10 |   test("should render the Graph tab with ReactFlow canvas", async ({
  11 |     page,
  12 |   }) => {
  13 |     // ReactFlow renders a .react-flow container
  14 |     const flow = page.locator(".react-flow");
  15 |     await expect(flow).toBeVisible({ timeout: 10_000 });
  16 |   });
  17 | 
  18 |   test("should display seed nodes in the graph", async ({ page }) => {
  19 |     // Wait for ReactFlow to render nodes
  20 |     const flow = page.locator(".react-flow");
  21 |     await expect(flow).toBeVisible({ timeout: 10_000 });
  22 | 
  23 |     // The graph panel seeds 3 demo nodes
  24 |     const nodes = page.locator(".react-flow__node");
  25 |     await expect(nodes).toHaveCount(3, { timeout: 5_000 });
  26 |   });
  27 | 
  28 |   test("should display seed edges in the graph", async ({ page }) => {
  29 |     const flow = page.locator(".react-flow");
  30 |     await expect(flow).toBeVisible({ timeout: 10_000 });
  31 | 
  32 |     // 2 edges: one hardRef, one weakRef
  33 |     const edges = page.locator(".react-flow__edge");
  34 |     await expect(edges).toHaveCount(2, { timeout: 5_000 });
  35 |   });
  36 | 
  37 |   test("should render custom node content", async ({ page }) => {
  38 |     const flow = page.locator(".react-flow");
  39 |     await expect(flow).toBeVisible({ timeout: 10_000 });
  40 | 
  41 |     // CodeMirror node should show "main.ts" header
  42 |     await expect(page.locator(".prism-node-header").first()).toBeVisible({
  43 |       timeout: 5_000,
  44 |     });
  45 |   });
  46 | 
  47 |   test("should navigate to Graph tab via KBar", async ({ page }) => {
  48 |     // Start on editor tab
  49 |     await page.getByRole("button", { name: "Editor" }).click();
  50 | 
  51 |     // Open KBar — try both Meta+k (macOS) and Control+k (headless fallback)
  52 |     await page.keyboard.press("Meta+k");
  53 |     const kbarInput = page.locator('[role="combobox"]');
  54 |     if (!(await kbarInput.isVisible({ timeout: 1_000 }).catch(() => false))) {
  55 |       await page.keyboard.press("Control+k");
  56 |     }
  57 |     await expect(kbarInput).toBeVisible({ timeout: 3_000 });
  58 |     await kbarInput.fill("Graph");
  59 | 
  60 |     // Select the action
  61 |     await page.keyboard.press("Enter");
  62 | 
  63 |     // Should now be on graph tab
  64 |     const flow = page.locator(".react-flow");
  65 |     await expect(flow).toBeVisible({ timeout: 10_000 });
  66 |   });
  67 | });
  68 | 
```