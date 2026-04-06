import { test, expect } from "@playwright/test";

test.describe("Studio Kernel: Object Explorer", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("object explorer renders with seed data", async ({ page }) => {
    const explorer = page.locator('[data-testid="object-explorer"]');
    await expect(explorer).toBeVisible();

    // Seed data includes at least "Home" and "About" pages
    await expect(explorer.locator("text=Home")).toBeVisible();
    await expect(explorer.locator("text=About")).toBeVisible();
  });

  test("clicking an object in the explorer selects it", async ({ page }) => {
    const explorer = page.locator('[data-testid="object-explorer"]');
    await expect(explorer).toBeVisible();

    // Click the "Home" object
    await explorer.locator("text=Home").first().click();

    // Inspector should show the selected object
    const inspector = page.locator('[data-testid="inspector-content"]');
    await expect(inspector).toBeVisible();
    await expect(inspector.locator("text=Inspector")).toBeVisible();
  });

  test("create page button adds a new page", async ({ page }) => {
    const createBtn = page.locator('[data-testid="create-page-btn"]');
    await expect(createBtn).toBeVisible();

    await createBtn.click();

    // A notification toast should appear
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });

    // The new page should appear in the explorer
    const explorer = page.locator('[data-testid="object-explorer"]');
    await expect(explorer.locator("text=New Page")).toBeVisible();
  });
});

test.describe("Studio Kernel: Inspector Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Select the "Home" page
    const explorer = page.locator('[data-testid="object-explorer"]');
    await expect(explorer).toBeVisible();
    await explorer.locator("text=Home").first().click();
  });

  test("inspector shows selected object properties", async ({ page }) => {
    const inspector = page.locator('[data-testid="inspector-content"]');
    await expect(inspector).toBeVisible();

    // Should show the name field with "Home"
    const nameInput = inspector.locator('input[type="text"]').first();
    await expect(nameInput).toHaveValue("Home");
  });

  test("editing name updates the object", async ({ page }) => {
    const inspector = page.locator('[data-testid="inspector-content"]');
    const nameInput = inspector.locator('input[type="text"]').first();

    // Clear and type a new name
    await nameInput.fill("Homepage");

    // The explorer should reflect the rename
    const explorer = page.locator('[data-testid="object-explorer"]');
    await expect(explorer.locator("text=Homepage")).toBeVisible();
  });

  test("inspector shows metadata section", async ({ page }) => {
    const inspector = page.locator('[data-testid="inspector-content"]');

    // Should show Metadata section with ID, Type, Created, Updated
    await expect(inspector.locator("text=Metadata")).toBeVisible();
    await expect(inspector.locator("text=Type: page")).toBeVisible();
  });

  test("inspector shows entity-specific fields", async ({ page }) => {
    const inspector = page.locator('[data-testid="inspector-content"]');

    // Page type should have title, slug, layout, published fields
    // These are rendered as field labels
    await expect(inspector.locator("text=title").first()).toBeVisible();
    await expect(inspector.locator("text=slug").first()).toBeVisible();
  });

  test("delete button removes object and deselects", async ({ page }) => {
    const deleteBtn = page.locator('[data-testid="delete-object-btn"]');
    await expect(deleteBtn).toBeVisible();

    await deleteBtn.click();

    // Inspector should show "Select an object" placeholder
    const inspector = page.locator('[data-testid="inspector"]');
    await expect(inspector.locator("text=Select an object")).toBeVisible();

    // A deletion notification toast should appear
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
  });
});

test.describe("Studio Kernel: Undo/Redo", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("undo button is initially disabled (seed data clears undo)", async ({ page }) => {
    const undoBtn = page.locator('[data-testid="undo-btn"]');
    await expect(undoBtn).toBeVisible();
    await expect(undoBtn).toBeDisabled();
  });

  test("redo button is initially disabled", async ({ page }) => {
    const redoBtn = page.locator('[data-testid="redo-btn"]');
    await expect(redoBtn).toBeVisible();
    await expect(redoBtn).toBeDisabled();
  });

  test("creating an object enables undo", async ({ page }) => {
    const createBtn = page.locator('[data-testid="create-page-btn"]');
    await createBtn.click();

    const undoBtn = page.locator('[data-testid="undo-btn"]');
    await expect(undoBtn).toBeEnabled();
  });

  test("undo reverses a create action", async ({ page }) => {
    const explorer = page.locator('[data-testid="object-explorer"]');
    const createBtn = page.locator('[data-testid="create-page-btn"]');

    await createBtn.click();
    await expect(explorer.locator("text=New Page")).toBeVisible();

    // Undo the creation
    const undoBtn = page.locator('[data-testid="undo-btn"]');
    await undoBtn.click();

    // The new page should be gone
    await expect(explorer.locator("text=New Page")).not.toBeVisible();
  });

  test("redo restores an undone action", async ({ page }) => {
    const explorer = page.locator('[data-testid="object-explorer"]');
    const createBtn = page.locator('[data-testid="create-page-btn"]');

    await createBtn.click();
    await expect(explorer.locator("text=New Page")).toBeVisible();

    // Undo
    const undoBtn = page.locator('[data-testid="undo-btn"]');
    await undoBtn.click();
    await expect(explorer.locator("text=New Page")).not.toBeVisible();

    // Redo
    const redoBtn = page.locator('[data-testid="redo-btn"]');
    await expect(redoBtn).toBeEnabled();
    await redoBtn.click();
    await expect(explorer.locator("text=New Page")).toBeVisible();
  });

  test("Cmd+Z triggers undo", async ({ page }) => {
    const explorer = page.locator('[data-testid="object-explorer"]');
    const createBtn = page.locator('[data-testid="create-page-btn"]');

    await createBtn.click();
    await expect(explorer.locator("text=New Page")).toBeVisible();

    const isMac = await page.evaluate(() =>
      /Mac|iPod|iPhone|iPad/.test(navigator.platform),
    );
    await page.keyboard.press(isMac ? "Meta+z" : "Control+z");

    await expect(explorer.locator("text=New Page")).not.toBeVisible();
  });
});

test.describe("Studio Kernel: Notifications", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("creating a page shows a success toast", async ({ page }) => {
    const createBtn = page.locator('[data-testid="create-page-btn"]');
    await createBtn.click();

    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
    await expect(toast.first()).toContainText("Created");
  });

  test("toast auto-dismisses after a few seconds", async ({ page }) => {
    const createBtn = page.locator('[data-testid="create-page-btn"]');
    await createBtn.click();

    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });

    // Should auto-dismiss (4s timeout in code)
    await expect(toast).not.toBeVisible({ timeout: 6000 });
  });
});

test.describe("Studio Kernel: Add Child", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Select "Home" page which should have "Add Child" options
    const explorer = page.locator('[data-testid="object-explorer"]');
    await expect(explorer).toBeVisible();
    await explorer.locator("text=Home").first().click();
  });

  test("inspector shows Add Child buttons for valid child types", async ({ page }) => {
    const inspector = page.locator('[data-testid="inspector-content"]');
    await expect(inspector.locator("text=Add Child")).toBeVisible();

    // Page can contain sections
    await expect(inspector.locator("button", { hasText: "Section" })).toBeVisible();
  });

  test("clicking Add Child creates a child and selects it", async ({ page }) => {
    const inspector = page.locator('[data-testid="inspector-content"]');
    const sectionBtn = inspector.locator("button", { hasText: "Section" });
    await sectionBtn.click();

    // Inspector should now show the new section
    await expect(inspector.locator("text=Type: section")).toBeVisible();

    // A notification toast should confirm creation
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
    await expect(toast.first()).toContainText("Created");
  });
});
