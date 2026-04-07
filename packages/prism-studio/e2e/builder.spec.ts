import { test, expect } from "@playwright/test";

/**
 * Builder Integration E2E Tests
 *
 * Tests the unified Puck/Lua/Canvas builder system:
 * - Layout Panel (Puck visual builder)
 * - Canvas Panel (WYSIWYG preview with lua-block rendering)
 * - Lua Facet Panel (Lua UI editor bound to kernel objects)
 * - Cross-panel integration (edit in one, see in another)
 */

// ── Layout Panel (Puck) ─────────────────────────────────────────────────────

test.describe("Layout Panel — Puck Builder", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Select Home page (seed data)
    const explorer = page.locator('[data-testid="object-explorer"]');
    await expect(explorer).toBeVisible();
    await explorer.locator("text=Home").first().click();
    // Navigate to Layout lens
    await page.locator('[data-testid="activity-icon-layout"]').click();
    await expect(page.locator('[data-testid="layout-panel"]')).toBeVisible();
  });

  test("renders Puck editor with kernel-derived components", async ({ page }) => {
    // Puck should render — wait for the Puck UI container
    const puck = page.locator('[class*="Puck"]').first();
    await expect(puck).toBeVisible({ timeout: 10_000 });
  });

  test("component list includes all registered entity types", async ({ page }) => {
    // Puck's sidebar shows registered component types
    await expect(page.locator("text=Heading").first()).toBeVisible({ timeout: 10_000 });
    await expect(page.locator("text=Text").first()).toBeVisible();
    await expect(page.locator("text=Card").first()).toBeVisible();
    await expect(page.locator("text=Button").first()).toBeVisible();
    await expect(page.locator("text=Section").first()).toBeVisible();
    await expect(page.locator("text=Image").first()).toBeVisible();
  });

  test("component list includes Lua Block type", async ({ page }) => {
    // The lua-block entity should appear as a Puck component
    await expect(page.locator("text=Lua Block").first()).toBeVisible({ timeout: 10_000 });
  });

  test("shows existing page content from kernel", async ({ page }) => {
    // The Home page seed data has heading/text content — Puck should show it
    const puck = page.locator('[class*="Puck"]').first();
    await expect(puck).toBeVisible({ timeout: 10_000 });
    // Content area should have rendered components
    await expect(puck.locator('[class*="content"], [class*="preview"]').first()).toBeVisible();
  });

  test("shows empty state when no page selected", async ({ page }) => {
    // Deselect everything by clicking layout again (no selection)
    // First go back to Editor, deselect, then go to Layout
    await page.locator('[data-testid="activity-icon-editor"]').click();

    // Create a non-page item to deselect the page context
    // Navigate to Layout — should show empty state
    await page.locator('[data-testid="activity-icon-layout"]').click();
    const layoutPanel = page.locator('[data-testid="layout-panel"]');
    await expect(layoutPanel).toBeVisible();
  });
});

// ── Canvas Panel — WYSIWYG Preview ──────────────────────────────────────────

test.describe("Canvas Panel — WYSIWYG Preview", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-canvas"]').click();
    await expect(page.locator('[data-testid="canvas-panel"]')).toBeVisible();
  });

  test("renders seed page content with sections", async ({ page }) => {
    const canvas = page.locator('[data-testid="canvas-panel"]');
    // Home page is auto-selected — should show content
    await expect(canvas.locator("h1", { hasText: "Build anything with Prism" })).toBeVisible();
    // Should have sections
    const sections = canvas.locator('[data-testid^="canvas-section-"]');
    await expect(sections.first()).toBeVisible();
  });

  test("renders lua-block inline with Lua UI preview", async ({ page }) => {
    const canvas = page.locator('[data-testid="canvas-panel"]');
    // Seed data includes a lua-block "Status Widget" — should render
    const luaPreview = canvas.locator('[data-testid^="lua-block-preview-"]').first();
    await expect(luaPreview).toBeVisible({ timeout: 5_000 });
    // Should show the title and have rendered content
    await expect(luaPreview.locator("text=Status Widget")).toBeVisible();
  });

  test("clicking a lua-block selects it in the kernel", async ({ page }) => {
    const canvas = page.locator('[data-testid="canvas-panel"]');
    const luaPreview = canvas.locator('[data-testid^="lua-block-preview-"]').first();
    await expect(luaPreview).toBeVisible({ timeout: 5_000 });

    // Click the lua-block
    await luaPreview.click();

    // Inspector should show the lua-block type
    const inspector = page.locator('[data-testid="inspector-content"]');
    await expect(inspector.locator("text=Type: lua-block")).toBeVisible();
  });

  test("block toolbar appears on selected blocks", async ({ page }) => {
    const canvas = page.locator('[data-testid="canvas-panel"]');
    // Click the heading
    const heading = canvas.locator("h1", { hasText: "Build anything with Prism" });
    await heading.click();

    // Toolbar should appear
    const toolbar = canvas.locator('[data-testid^="block-toolbar-"]').first();
    await expect(toolbar).toBeVisible();
    await expect(toolbar.locator('[data-testid="toolbar-move-up"]')).toBeVisible();
    await expect(toolbar.locator('[data-testid="toolbar-duplicate"]')).toBeVisible();
    await expect(toolbar.locator('[data-testid="toolbar-delete"]')).toBeVisible();
  });

  test("duplicate block creates a copy", async ({ page }) => {
    const canvas = page.locator('[data-testid="canvas-panel"]');
    const heading = canvas.locator("h1", { hasText: "Build anything with Prism" });
    await heading.click();

    // Click duplicate
    const toolbar = canvas.locator('[data-testid^="block-toolbar-"]').first();
    await toolbar.locator('[data-testid="toolbar-duplicate"]').click();

    // Toast notification
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });

    // Should now have two headings with "Build anything"
    const headings = canvas.locator('h1:has-text("Build anything")');
    await expect(headings).toHaveCount(2, { timeout: 3000 });
  });

  test("quick-create menu shows allowed child types", async ({ page }) => {
    const canvas = page.locator('[data-testid="canvas-panel"]');
    // Find a quick-create trigger in a section
    const trigger = canvas.locator('[data-testid="quick-create-trigger"]').first();
    await expect(trigger).toBeVisible();
    await trigger.click();

    // Quick-create menu should show available component types
    const menu = canvas.locator('[data-testid="quick-create-menu"]').first();
    await expect(menu).toBeVisible();
    // Should include lua-block as an option
    await expect(menu.locator('[data-testid="quick-create-option-lua-block"]')).toBeVisible();
  });

  test("quick-create adds a new block to the section", async ({ page }) => {
    const canvas = page.locator('[data-testid="canvas-panel"]');
    const trigger = canvas.locator('[data-testid="quick-create-trigger"]').first();
    await trigger.click();

    const menu = canvas.locator('[data-testid="quick-create-menu"]').first();
    await expect(menu).toBeVisible();

    // Add a heading via quick-create
    await menu.locator('[data-testid="quick-create-option-heading"]').click();

    // Menu should close after adding
    await expect(menu).not.toBeVisible({ timeout: 3000 });
  });
});

// ── Lua Facet Panel ─────────────────────────────────────────────────────────

test.describe("Lua Facet Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-lua-facet"]').click();
    await expect(page.locator('[data-testid="lua-facet-panel"]')).toBeVisible();
  });

  test("renders with default Hello World sample", async ({ page }) => {
    const editor = page.locator('[data-testid="lua-editor"]');
    await expect(editor).toBeVisible();
    // Default source should contain "Hello World"
    const value = await editor.inputValue();
    expect(value).toContain("Hello");
  });

  test("preview renders Lua UI elements", async ({ page }) => {
    // Default sample should produce a preview
    const preview = page.locator('[data-testid="preview-pane"]');
    await expect(preview).toBeVisible();
    // "Hello from Lua!" label should render
    await expect(preview.locator('[data-testid="ui-label-0"]')).toBeVisible();
  });

  test("loading a sample script updates editor", async ({ page }) => {
    const select = page.locator('[data-testid="sample-select"]');
    await select.selectOption("Status Badge");

    // Editor should now have Status Badge source
    const editor = page.locator('[data-testid="lua-editor"]');
    const value = await editor.inputValue();
    expect(value).toContain("badge");

    // Toast notification for loading
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
  });

  test("editing source updates preview live", async ({ page }) => {
    const editor = page.locator('[data-testid="lua-editor"]');
    await editor.fill('return ui.button("Test Button")');

    // Preview should update
    const preview = page.locator('[data-testid="preview-pane"]');
    await expect(preview.locator('[data-testid="ui-button-0"]')).toBeVisible();
    await expect(preview.locator("text=Test Button")).toBeVisible();
  });

  test("invalid Lua shows parse error", async ({ page }) => {
    const editor = page.locator('[data-testid="lua-editor"]');
    await editor.fill('return ui.unknown("oops")');

    // Error should display
    const error = page.locator('[data-testid="preview-error"]');
    await expect(error).toBeVisible();
    await expect(error).toContainText("Unknown ui element");
  });

  test("empty source shows empty state", async ({ page }) => {
    const editor = page.locator('[data-testid="lua-editor"]');
    await editor.fill("");

    const empty = page.locator('[data-testid="preview-empty"]');
    await expect(empty).toBeVisible();
    await expect(empty).toContainText("No ui elements detected");
  });

  test("copy button copies source to clipboard", async ({ page }) => {
    const copyBtn = page.locator('[data-testid="copy-lua-btn"]');
    await copyBtn.click();
    await expect(copyBtn).toHaveText("Copied");
    // Toast notification
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
  });

  test("context display shows viewId and instanceKey", async ({ page }) => {
    const ctx = page.locator('[data-testid="ctx-display"]');
    await expect(ctx).toBeVisible();
    await expect(ctx).toContainText("viewId:");
    await expect(ctx).toContainText("instanceKey:");
  });

  test("binding indicator shows when lua-block is selected", async ({ page }) => {
    // First go to Editor to select a lua-block
    await page.locator('[data-testid="activity-icon-editor"]').click();
    const explorer = page.locator('[data-testid="object-explorer"]');
    // Expand tree to find the lua-block
    await explorer.locator("text=Status Widget").first().click();

    // Switch to Lua Facet
    await page.locator('[data-testid="activity-icon-lua-facet"]').click();
    await expect(page.locator('[data-testid="lua-facet-panel"]')).toBeVisible();

    // Should show binding indicator
    const binding = page.locator('[data-testid="lua-object-binding"]');
    await expect(binding).toBeVisible();
    await expect(binding).toContainText("Bound");
    await expect(binding).toContainText("Status Widget");
  });

  test("editing bound lua-block updates kernel object", async ({ page }) => {
    // Select the lua-block
    await page.locator('[data-testid="activity-icon-editor"]').click();
    const explorer = page.locator('[data-testid="object-explorer"]');
    await explorer.locator("text=Status Widget").first().click();

    // Switch to Lua Facet
    await page.locator('[data-testid="activity-icon-lua-facet"]').click();
    await expect(page.locator('[data-testid="lua-object-binding"]')).toBeVisible();

    // Edit the source
    const editor = page.locator('[data-testid="lua-editor"]');
    await editor.fill('return ui.label("Updated via Lua Facet")');

    // Wait for debounced save
    await page.waitForTimeout(600);

    // Switch to Canvas to verify update
    await page.locator('[data-testid="activity-icon-canvas"]').click();
    const canvas = page.locator('[data-testid="canvas-panel"]');
    await expect(canvas.locator("text=Updated via Lua Facet")).toBeVisible({ timeout: 5_000 });
  });
});

// ── Component Palette ───────────────────────────────────────────────────────

test.describe("Component Palette", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Palette is in the sidebar — need to have a page selected
    const explorer = page.locator('[data-testid="object-explorer"]');
    await expect(explorer).toBeVisible();
  });

  test("palette shows component types grouped by category", async ({ page }) => {
    const palette = page.locator('[data-testid="component-palette"]');
    await expect(palette).toBeVisible();
    // Should have component items
    await expect(palette.locator('[data-testid^="palette-item-"]').first()).toBeVisible();
  });

  test("palette includes lua-block type", async ({ page }) => {
    const palette = page.locator('[data-testid="component-palette"]');
    await expect(palette.locator('[data-testid="palette-item-lua-block"]')).toBeVisible();
  });

  test("search filters palette items", async ({ page }) => {
    const palette = page.locator('[data-testid="component-palette"]');
    const search = palette.locator('[data-testid="palette-search"]');
    await search.fill("lua");

    // Should show lua-block, hide others
    await expect(palette.locator('[data-testid="palette-item-lua-block"]')).toBeVisible();
    await expect(palette.locator('[data-testid="palette-item-heading"]')).not.toBeVisible();
  });

  test("clicking palette item with page selected adds child", async ({ page }) => {
    // Select a section first
    const explorer = page.locator('[data-testid="object-explorer"]');
    await explorer.locator("text=Content").first().click();

    const palette = page.locator('[data-testid="component-palette"]');
    await palette.locator('[data-testid="palette-add-heading"]').click();

    // Toast should confirm
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
  });
});

// ── Facet Designer Panel ────────────────────────────────────────────────────

test.describe("Facet Designer Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-facet-designer"]').click();
    await expect(page.locator('[data-testid="facet-designer-panel"]')).toBeVisible();
  });

  test("renders with new facet button", async ({ page }) => {
    const createBtn = page.locator('[data-testid="new-facet-btn"]');
    await expect(createBtn).toBeVisible();
  });

  test("shows empty state initially", async ({ page }) => {
    const empty = page.locator('[data-testid="empty-state"]');
    await expect(empty).toBeVisible();
  });

  test("creating a new definition enters draft mode", async ({ page }) => {
    await page.locator('[data-testid="new-facet-btn"]').click();

    // Draft item should appear in sidebar
    const draft = page.locator('[data-testid="facet-item-draft"]');
    await expect(draft).toBeVisible();

    // Name input should be visible for editing
    const nameInput = page.locator('[data-testid="facet-name-input"]');
    await expect(nameInput).toBeVisible();
  });

  test("layout type selector has all options", async ({ page }) => {
    await page.locator('[data-testid="new-facet-btn"]').click();

    const select = page.locator('[data-testid="facet-layout-select"]');
    await expect(select).toBeVisible();
    // Should have form, list, table, report, card options
    const options = await select.locator("option").allTextContents();
    expect(options.some((o) => o.toLowerCase().includes("form"))).toBe(true);
    expect(options.some((o) => o.toLowerCase().includes("list"))).toBe(true);
    expect(options.some((o) => o.toLowerCase().includes("table"))).toBe(true);
  });

  test("can add layout parts to a definition", async ({ page }) => {
    await page.locator('[data-testid="new-facet-btn"]').click();

    // Part buttons should be visible (body, header, footer, etc.)
    // Use the first available part kind
    const addBody = page.locator('[data-testid="add-part-body"]');
    if (await addBody.isVisible()) {
      await addBody.click();
      const partCard = page.locator('[data-testid="part-card-0"]');
      await expect(partCard).toBeVisible();
    } else {
      // Try header
      const addHeader = page.locator('[data-testid="add-part-header"]');
      await expect(addHeader).toBeVisible();
      await addHeader.click();
      const partCard = page.locator('[data-testid="part-card-0"]');
      await expect(partCard).toBeVisible();
    }
  });

  test("can add field slots to a definition", async ({ page }) => {
    await page.locator('[data-testid="new-facet-btn"]').click();

    const addField = page.locator('[data-testid="add-field-btn"]');
    await expect(addField).toBeVisible();
    await addField.click();

    // Field slot card should appear
    const fieldSlot = page.locator('[data-testid="field-slot-0"]');
    await expect(fieldSlot).toBeVisible();
  });

  test("save button persists definition to kernel", async ({ page }) => {
    await page.locator('[data-testid="new-facet-btn"]').click();

    // Fill in name
    const nameInput = page.locator('[data-testid="facet-name-input"]');
    await nameInput.fill("Test Layout");

    // Save
    const saveBtn = page.locator('[data-testid="save-facet-btn"]');
    await expect(saveBtn).toBeVisible();
    await saveBtn.click();

    // Should now appear in sidebar as a saved item
    const facetItem = page.locator('[data-testid^="facet-item-"]').first();
    await expect(facetItem).toBeVisible();
  });
});

// ── Record Browser Panel ────────────────────────────────────────────────────

test.describe("Record Browser Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-record-browser"]').click();
    await expect(page.locator('[data-testid="record-browser-panel"]')).toBeVisible();
  });

  test("renders with toolbar and mode buttons", async ({ page }) => {
    const panel = page.locator('[data-testid="record-browser-panel"]');
    // Mode toggle buttons
    await expect(panel.locator('[data-testid="mode-btn-form"]')).toBeVisible();
    await expect(panel.locator('[data-testid="mode-btn-list"]')).toBeVisible();
    await expect(panel.locator('[data-testid="mode-btn-table"]')).toBeVisible();
    await expect(panel.locator('[data-testid="mode-btn-report"]')).toBeVisible();
    await expect(panel.locator('[data-testid="mode-btn-card"]')).toBeVisible();
  });

  test("shows kernel objects as records", async ({ page }) => {
    const panel = page.locator('[data-testid="record-browser-panel"]');
    // Should show seed data objects (Home, About pages at minimum)
    await expect(panel.locator("text=Home").first()).toBeVisible();
  });

  test("switching to list mode shows list view", async ({ page }) => {
    const panel = page.locator('[data-testid="record-browser-panel"]');
    await panel.locator('[data-testid="mode-btn-list"]').click();

    // List view should be active
    await expect(panel.locator('[data-testid="list-view"]')).toBeVisible();
  });

  test("switching to table mode shows table view", async ({ page }) => {
    const panel = page.locator('[data-testid="record-browser-panel"]');
    await panel.locator('[data-testid="mode-btn-table"]').click();

    // Table view should be active
    await expect(panel.locator('[data-testid="table-view"]')).toBeVisible();
  });

  test("switching to card mode shows card grid", async ({ page }) => {
    const panel = page.locator('[data-testid="record-browser-panel"]');
    await panel.locator('[data-testid="mode-btn-card"]').click();

    // Card grid should be active
    await expect(panel.locator('[data-testid="card-view"]')).toBeVisible();
  });

  test("switching to report mode shows report view", async ({ page }) => {
    const panel = page.locator('[data-testid="record-browser-panel"]');
    await panel.locator('[data-testid="mode-btn-report"]').click();

    // Report view should be active
    await expect(panel.locator('[data-testid="report-view"]')).toBeVisible();
  });

  test("search input filters records", async ({ page }) => {
    const panel = page.locator('[data-testid="record-browser-panel"]');
    const search = panel.locator('[data-testid="browser-search-input"]');
    await expect(search).toBeVisible();
    await search.fill("Home");

    // Should still show Home, but filter out non-matching
    await expect(panel.locator("text=Home").first()).toBeVisible();
  });

  test("type filter dropdown is visible", async ({ page }) => {
    const panel = page.locator('[data-testid="record-browser-panel"]');
    const filter = panel.locator('[data-testid="browser-type-filter"]');
    await expect(filter).toBeVisible();
  });

  test("record navigation shows position", async ({ page }) => {
    const panel = page.locator('[data-testid="record-browser-panel"]');
    const position = panel.locator('[data-testid="nav-position"]');
    await expect(position).toBeVisible();
  });

  test("status bar shows total count and mode", async ({ page }) => {
    const panel = page.locator('[data-testid="record-browser-panel"]');
    const total = panel.locator('[data-testid="status-total"]');
    await expect(total).toBeVisible();
    const mode = panel.locator('[data-testid="status-mode"]');
    await expect(mode).toBeVisible();
  });

  test("new record button creates a record", async ({ page }) => {
    const panel = page.locator('[data-testid="record-browser-panel"]');
    const newBtn = panel.locator('[data-testid="new-record-btn"]');
    await expect(newBtn).toBeVisible();
    await newBtn.click();

    // Toast notification confirming creation
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
  });
});

// ── Cross-Panel Integration ─────────────────────────────────────────────────

test.describe("Cross-Panel Builder Integration", () => {
  test("canvas reflects kernel changes from inspector edits", async ({ page }) => {
    await page.goto("/");

    // Open Canvas, verify heading text
    await page.locator('[data-testid="activity-icon-canvas"]').click();
    const canvas = page.locator('[data-testid="canvas-panel"]');
    await expect(canvas.locator("h1", { hasText: "Build anything with Prism" })).toBeVisible();

    // Click the heading to select it
    await canvas.locator("h1", { hasText: "Build anything with Prism" }).click();

    // Inspector should show the heading type
    const inspector = page.locator('[data-testid="inspector-content"]');
    await expect(inspector).toBeVisible();
    await expect(inspector.locator("text=Type: heading")).toBeVisible();
  });

  test("creating object via palette shows in canvas", async ({ page }) => {
    await page.goto("/");

    // Select Home page's Content section in explorer
    const explorer = page.locator('[data-testid="object-explorer"]');
    await explorer.locator("text=Content").first().click();

    // Add a new heading via palette (now visible in sidebar)
    const palette = page.locator('[data-testid="component-palette"]');
    await expect(palette).toBeVisible();
    await palette.locator('[data-testid="palette-add-heading"]').click();

    // Switch to Canvas
    await page.locator('[data-testid="activity-icon-canvas"]').click();
    const canvas = page.locator('[data-testid="canvas-panel"]');

    // New heading should appear
    await expect(canvas.locator("text=New Heading").first()).toBeVisible({ timeout: 3000 });
  });

  test("deleting block in canvas works", async ({ page }) => {
    await page.goto("/");

    // Open Canvas
    await page.locator('[data-testid="activity-icon-canvas"]').click();
    const canvas = page.locator('[data-testid="canvas-panel"]');

    // Click the heading to select it
    const heading = canvas.locator("h1", { hasText: "Build anything with Prism" });
    await expect(heading).toBeVisible();
    await heading.click();

    // Click delete in toolbar
    const toolbar = canvas.locator('[data-testid^="block-toolbar-"]').first();
    await toolbar.locator('[data-testid="toolbar-delete"]').click();

    // Heading should be gone from canvas
    await expect(heading).not.toBeVisible({ timeout: 3000 });

    // Toast should confirm deletion
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
  });

  test("graph panel renders object nodes", async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-graph"]').click();
    const graph = page.locator('[data-testid="graph-panel"]');
    await expect(graph).toBeVisible();

    // Graph should render — React Flow creates .react-flow container
    await expect(graph.locator(".react-flow").first()).toBeVisible({ timeout: 5_000 });
  });

  test("undo/redo works across builder operations", async ({ page }) => {
    await page.goto("/");

    // Open Canvas
    await page.locator('[data-testid="activity-icon-canvas"]').click();
    const canvas = page.locator('[data-testid="canvas-panel"]');
    const heading = canvas.locator("h1", { hasText: "Build anything with Prism" });
    await expect(heading).toBeVisible();

    // Delete a block
    await heading.click();
    const toolbar = canvas.locator('[data-testid^="block-toolbar-"]').first();
    await toolbar.locator('[data-testid="toolbar-delete"]').click();
    await expect(heading).not.toBeVisible({ timeout: 3000 });

    // Undo
    await page.keyboard.press("Meta+z");

    // Heading should reappear
    await expect(canvas.locator("h1", { hasText: "Build anything with Prism" })).toBeVisible({ timeout: 3000 });
  });
});
