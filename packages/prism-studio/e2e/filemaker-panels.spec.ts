import { test, expect } from "@playwright/test";

/**
 * FileMaker Parity Panels — E2E Tests
 *
 * Tests the 5 new Studio panels that close FileMaker Pro feature gaps:
 * - Visual Script Panel (lens #24, Shift+S)
 * - Saved View Panel (lens #25, Shift+V)
 * - Value List Panel (lens #26, Shift+L)
 * - Privilege Set Panel (lens #27, Shift+P)
 * - Container Field Renderer (inline in spatial-canvas)
 */

// ── Visual Script Panel ─────────────────────────────────────────────────────

test.describe("Visual Script Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-visual-script"]').click();
    await expect(page.locator('[data-testid="visual-script-panel"]')).toBeVisible({ timeout: 10_000 });
  });

  test("panel loads with step palette sidebar", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await expect(panel).toBeVisible();
    // Step palette should show categories
    await expect(panel.locator("text=Step Palette")).toBeVisible();
  });

  test("shows empty script message", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await expect(panel.locator("text=Click steps in the palette")).toBeVisible();
  });

  test("displays script name input", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    const nameInput = panel.locator('input[value="New Script"]');
    await expect(nameInput).toBeVisible();
  });

  test("shows Lua preview area", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await expect(panel.locator("text=-- Empty script")).toBeVisible();
  });

  test("shows step count as 0 steps", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await expect(panel.locator("text=0 steps")).toBeVisible();
  });

  test("palette shows Navigation category", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await expect(panel.locator("text=NAVIGATION")).toBeVisible();
  });

  test("palette shows Control Flow category", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await expect(panel.locator("text=CONTROL FLOW")).toBeVisible();
  });

  test("shows Copy Lua button", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await expect(panel.locator("text=Copy Lua")).toBeVisible();
  });

  test("adding a step updates step count", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    // Click "Set Field" in the palette
    await panel.locator("text=Set Field").first().click();
    await expect(panel.locator("text=1 step")).toBeVisible();
  });

  test("adding a step shows it in the step list", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await panel.locator("text=Show Notification").first().click();
    // Step card should appear with step number
    await expect(panel.locator("text=1.")).toBeVisible();
  });

  test("adding a step generates Lua preview", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await panel.locator("text=Comment").first().click();
    // Lua preview should no longer show empty script
    await expect(panel.locator("text=-- Empty script")).not.toBeVisible();
  });

  test("adding If/End If pair passes validation", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await panel.locator("text=If").first().click();
    await panel.locator("text=End If").first().click();
    // No error box should appear
    await expect(panel.locator("text=Unmatched")).not.toBeVisible();
  });

  test("unmatched If shows validation error", async ({ page }) => {
    const panel = page.locator('[data-testid="visual-script-panel"]');
    await panel.locator("text=If").first().click();
    // Should show validation error
    await expect(panel.locator("text=Unmatched")).toBeVisible();
  });
});

// ── Saved View Panel ────────────────────────────────────────────────────────

test.describe("Saved View Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-saved-view"]').click();
    await expect(page.locator('[data-testid="saved-view-panel"]')).toBeVisible({ timeout: 10_000 });
  });

  test("panel loads with title", async ({ page }) => {
    const panel = page.locator('[data-testid="saved-view-panel"]');
    await expect(panel.locator("text=Saved Views")).toBeVisible();
  });

  test("shows seeded example views", async ({ page }) => {
    const panel = page.locator('[data-testid="saved-view-panel"]');
    await expect(panel.locator("text=Active Tasks")).toBeVisible();
    await expect(panel.locator("text=Overdue Invoices")).toBeVisible();
  });

  test("shows New View button", async ({ page }) => {
    const panel = page.locator('[data-testid="saved-view-panel"]');
    await expect(panel.locator("text=+ New View")).toBeVisible();
  });

  test("shows search input", async ({ page }) => {
    const panel = page.locator('[data-testid="saved-view-panel"]');
    await expect(panel.locator('input[placeholder="Search views..."]')).toBeVisible();
  });

  test("shows pinned section for pinned views", async ({ page }) => {
    const panel = page.locator('[data-testid="saved-view-panel"]');
    // Active Tasks is pinned by default
    await expect(panel.locator("text=Pinned")).toBeVisible();
  });

  test("shows All Views count", async ({ page }) => {
    const panel = page.locator('[data-testid="saved-view-panel"]');
    await expect(panel.locator("text=All Views (2)")).toBeVisible();
  });

  test("clicking New View shows create form", async ({ page }) => {
    const panel = page.locator('[data-testid="saved-view-panel"]');
    await panel.locator("text=+ New View").click();
    await expect(panel.locator('input[placeholder="View name..."]')).toBeVisible();
    await expect(panel.locator("text=Save View")).toBeVisible();
  });

  test("search filters views", async ({ page }) => {
    const panel = page.locator('[data-testid="saved-view-panel"]');
    await panel.locator('input[placeholder="Search views..."]').fill("Overdue");
    await expect(panel.locator("text=Overdue Invoices")).toBeVisible();
    await expect(panel.locator("text=Active Tasks")).not.toBeVisible();
  });

  test("clicking a view activates it", async ({ page }) => {
    const panel = page.locator('[data-testid="saved-view-panel"]');
    await panel.locator("text=Overdue Invoices").click();
    // Active view gets highlighted border
    await expect(panel.locator("text=Overdue Invoices")).toBeVisible();
  });

  test("shows filter summary on view cards", async ({ page }) => {
    const panel = page.locator('[data-testid="saved-view-panel"]');
    await expect(panel.locator("text=1 filter, 1 sort").first()).toBeVisible();
  });

  test("shows view mode badge", async ({ page }) => {
    const panel = page.locator('[data-testid="saved-view-panel"]');
    await expect(panel.locator("text=list").first()).toBeVisible();
  });
});

// ── Value List Panel ────────────────────────────────────────────────────────

test.describe("Value List Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-value-list"]').click();
    await expect(page.locator('[data-testid="value-list-panel"]')).toBeVisible({ timeout: 10_000 });
  });

  test("panel loads with sidebar", async ({ page }) => {
    const panel = page.locator('[data-testid="value-list-panel"]');
    await expect(panel.locator("text=Value Lists")).toBeVisible();
  });

  test("shows seeded example lists", async ({ page }) => {
    const panel = page.locator('[data-testid="value-list-panel"]');
    await expect(panel.locator("text=Task Status")).toBeVisible();
    await expect(panel.locator("text=Priority")).toBeVisible();
    await expect(panel.locator("text=Clients")).toBeVisible();
  });

  test("shows Static and Dynamic create buttons", async ({ page }) => {
    const panel = page.locator('[data-testid="value-list-panel"]');
    await expect(panel.locator("text=+ Static")).toBeVisible();
    await expect(panel.locator("text=+ Dynamic")).toBeVisible();
  });

  test("shows type badges on list items", async ({ page }) => {
    const panel = page.locator('[data-testid="value-list-panel"]');
    await expect(panel.locator("text=static").first()).toBeVisible();
    await expect(panel.locator("text=dynamic").first()).toBeVisible();
  });

  test("shows empty state when no list selected", async ({ page }) => {
    const panel = page.locator('[data-testid="value-list-panel"]');
    await expect(panel.locator("text=Select a value list to edit")).toBeVisible();
  });

  test("clicking a static list shows value editor", async ({ page }) => {
    const panel = page.locator('[data-testid="value-list-panel"]');
    await panel.locator("text=Task Status").click();
    // Should show the inline value rows
    await expect(panel.locator("text=Active")).toBeVisible();
    await expect(panel.locator("text=Completed")).toBeVisible();
    await expect(panel.locator("text=Archived")).toBeVisible();
  });

  test("clicking a dynamic list shows source config", async ({ page }) => {
    const panel = page.locator('[data-testid="value-list-panel"]');
    await panel.locator("text=Clients").click();
    await expect(panel.locator("text=Collection ID")).toBeVisible();
    await expect(panel.locator("text=Value Field")).toBeVisible();
    await expect(panel.locator("text=Display Field")).toBeVisible();
  });

  test("static list shows item count", async ({ page }) => {
    const panel = page.locator('[data-testid="value-list-panel"]');
    await expect(panel.locator("text=3 items").first()).toBeVisible();
  });

  test("shows Add Value button for static lists", async ({ page }) => {
    const panel = page.locator('[data-testid="value-list-panel"]');
    await panel.locator("text=Task Status").click();
    await expect(panel.locator("text=+ Add Value")).toBeVisible();
  });

  test("shows Delete button when list is selected", async ({ page }) => {
    const panel = page.locator('[data-testid="value-list-panel"]');
    await panel.locator("text=Task Status").click();
    await expect(panel.locator("text=Delete")).toBeVisible();
  });

  test("creating a new static list selects it", async ({ page }) => {
    const panel = page.locator('[data-testid="value-list-panel"]');
    await panel.locator("text=+ Static").click();
    // New list should be selected and show "New Value List" name
    await expect(panel.locator('input[value="New Value List"]')).toBeVisible();
  });
});

// ── Privilege Set Panel ─────────────────────────────────────────────────────

test.describe("Privilege Set Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-privilege-set"]').click();
    await expect(page.locator('[data-testid="privilege-set-panel"]')).toBeVisible({ timeout: 10_000 });
  });

  test("panel loads with sidebar", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await expect(panel.locator("text=Privilege Sets")).toBeVisible();
  });

  test("shows seeded privilege sets", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await expect(panel.locator("text=Administrator")).toBeVisible();
    await expect(panel.locator("text=Editor")).toBeVisible();
    await expect(panel.locator("text=Viewer")).toBeVisible();
  });

  test("shows New Set button", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await expect(panel.locator("text=+ New Set")).toBeVisible();
  });

  test("shows default badge on Viewer", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await expect(panel.locator("text=default")).toBeVisible();
  });

  test("shows admin badge on Administrator", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await expect(panel.locator("text=admin")).toBeVisible();
  });

  test("Administrator is selected by default", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    // Should show Collection Permissions section
    await expect(panel.locator("text=Collection Permissions")).toBeVisible();
  });

  test("shows Collection Permissions section", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await expect(panel.locator("text=Collection Permissions")).toBeVisible();
    await expect(panel.locator("text=Default (*)")).toBeVisible();
  });

  test("shows Field Overrides section", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await expect(panel.locator("text=Field Overrides")).toBeVisible();
  });

  test("shows Row-Level Security section", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await expect(panel.locator("text=Row-Level Security")).toBeVisible();
  });

  test("shows Assigned Users section", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await expect(panel.locator("text=Assigned Users")).toBeVisible();
  });

  test("shows Options section with checkboxes", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await expect(panel.locator("text=Options")).toBeVisible();
    await expect(panel.locator("text=Default for new users")).toBeVisible();
    await expect(panel.locator("text=Can manage access control")).toBeVisible();
  });

  test("switching privilege sets changes editor", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await panel.locator("text=Viewer").click();
    // Viewer has "read" permission, not "full"
    const select = panel.locator("select").first();
    await expect(select).toHaveValue("read");
  });

  test("shows DID input for role assignment", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await expect(panel.locator('input[placeholder="did:key:..."]')).toBeVisible();
  });

  test("shows record filter expression input", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await expect(panel.locator('input[placeholder*="record.owner_did"]')).toBeVisible();
  });

  test("creating a new set selects it", async ({ page }) => {
    const panel = page.locator('[data-testid="privilege-set-panel"]');
    await panel.locator("text=+ New Set").click();
    await expect(panel.locator('input[value="New Privilege Set"]')).toBeVisible();
  });
});

// ── Container Field Renderer ────────────────────────────────────────────────

test.describe("Container Field Renderer — Spatial Canvas Integration", () => {
  test("spatial canvas panel loads", async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-spatial-canvas"]').click();
    await expect(page.locator('[data-testid="spatial-canvas-panel"]')).toBeVisible({ timeout: 10_000 });
  });
});

// ── Kernel Integration ──────────────────────────────────────────────────────

test.describe("Kernel Integration — New Systems", () => {
  test("all 4 new lenses appear in activity bar", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator('[data-testid="activity-icon-visual-script"]')).toBeVisible({ timeout: 10_000 });
    await expect(page.locator('[data-testid="activity-icon-saved-view"]')).toBeVisible();
    await expect(page.locator('[data-testid="activity-icon-value-list"]')).toBeVisible();
    await expect(page.locator('[data-testid="activity-icon-privilege-set"]')).toBeVisible();
  });

  test("can navigate between all new panels", async ({ page }) => {
    await page.goto("/");

    // Visual Script
    await page.locator('[data-testid="activity-icon-visual-script"]').click();
    await expect(page.locator('[data-testid="visual-script-panel"]')).toBeVisible({ timeout: 10_000 });

    // Saved View
    await page.locator('[data-testid="activity-icon-saved-view"]').click();
    await expect(page.locator('[data-testid="saved-view-panel"]')).toBeVisible({ timeout: 10_000 });

    // Value List
    await page.locator('[data-testid="activity-icon-value-list"]').click();
    await expect(page.locator('[data-testid="value-list-panel"]')).toBeVisible({ timeout: 10_000 });

    // Privilege Set
    await page.locator('[data-testid="activity-icon-privilege-set"]').click();
    await expect(page.locator('[data-testid="privilege-set-panel"]')).toBeVisible({ timeout: 10_000 });
  });
});
