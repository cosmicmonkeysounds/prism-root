import { test, expect } from "@playwright/test";

/**
 * Plugin Panel E2E Tests — Work, Finance, CRM, Life, Assets, Platform.
 *
 * Each plugin bundle registers entity types, a PrismPlugin, and a lens.
 * These tests verify the panels render, tabs work, and CRUD operations function.
 */

// ── Work Panel ──────────────────────────────────────────────────────────────

test.describe("Work Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-work"]').click();
    await expect(page.locator('[data-testid="work-panel"]')).toBeVisible({ timeout: 10_000 });
  });

  test("panel renders with tabs", async ({ page }) => {
    await expect(page.locator('[data-testid="work-tab-gigs"]')).toBeVisible();
    await expect(page.locator('[data-testid="work-tab-time"]')).toBeVisible();
    await expect(page.locator('[data-testid="work-tab-focus"]')).toBeVisible();
  });

  test("create and delete a gig", async ({ page }) => {
    await page.locator('[data-testid="work-new-gig"]').click();
    const card = page.locator('[data-testid^="work-gig-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Gig")).toBeVisible();
    await card.locator("text=Delete").click();
    await expect(card).not.toBeVisible({ timeout: 3000 });
  });

  test("create a time entry", async ({ page }) => {
    await page.locator('[data-testid="work-tab-time"]').click();
    await page.locator('[data-testid="work-new-time"]').click();
    const card = page.locator('[data-testid^="work-time-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=Time Entry")).toBeVisible();
  });

  test("create a focus block", async ({ page }) => {
    await page.locator('[data-testid="work-tab-focus"]').click();
    await page.locator('[data-testid="work-new-focus"]').click();
    const card = page.locator('[data-testid^="work-focus-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=Focus Block")).toBeVisible();
  });

  test("shows empty states", async ({ page }) => {
    await expect(page.locator("text=No gigs yet")).toBeVisible();
    await page.locator('[data-testid="work-tab-time"]').click();
    await expect(page.locator("text=No time entries")).toBeVisible();
    await page.locator('[data-testid="work-tab-focus"]').click();
    await expect(page.locator("text=No focus blocks")).toBeVisible();
  });
});

// ── Finance Panel ───────────────────────────────────────────────────────────

test.describe("Finance Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-finance"]').click();
    await expect(page.locator('[data-testid="finance-panel"]')).toBeVisible({ timeout: 10_000 });
  });

  test("panel renders with tabs", async ({ page }) => {
    await expect(page.locator('[data-testid="finance-tab-loans"]')).toBeVisible();
    await expect(page.locator('[data-testid="finance-tab-grants"]')).toBeVisible();
    await expect(page.locator('[data-testid="finance-tab-budgets"]')).toBeVisible();
  });

  test("create and delete a loan", async ({ page }) => {
    await page.locator('[data-testid="finance-new-loan"]').click();
    const card = page.locator('[data-testid^="finance-loan-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Loan")).toBeVisible();
    await card.locator("text=Delete").click();
    await expect(card).not.toBeVisible({ timeout: 3000 });
  });

  test("create a grant", async ({ page }) => {
    await page.locator('[data-testid="finance-tab-grants"]').click();
    await page.locator('[data-testid="finance-new-grant"]').click();
    const card = page.locator('[data-testid^="finance-grant-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Grant")).toBeVisible();
  });

  test("create a budget", async ({ page }) => {
    await page.locator('[data-testid="finance-tab-budgets"]').click();
    await page.locator('[data-testid="finance-new-budget"]').click();
    const card = page.locator('[data-testid^="finance-budget-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Budget")).toBeVisible();
  });
});

// ── CRM Panel ───────────────────────────────────────────────────────────────

test.describe("CRM Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-crm"]').click();
    await expect(page.locator('[data-testid="crm-panel"]')).toBeVisible({ timeout: 10_000 });
  });

  test("panel renders with tabs", async ({ page }) => {
    await expect(page.locator('[data-testid="crm-tab-contacts"]')).toBeVisible();
    await expect(page.locator('[data-testid="crm-tab-orgs"]')).toBeVisible();
    await expect(page.locator('[data-testid="crm-tab-pipeline"]')).toBeVisible();
  });

  test("create and delete a contact", async ({ page }) => {
    await page.locator('[data-testid="crm-new-contact"]').click();
    const card = page.locator('[data-testid^="crm-contact-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Contact")).toBeVisible();
    await card.locator("text=Delete").click();
    await expect(card).not.toBeVisible({ timeout: 3000 });
  });

  test("create an organization", async ({ page }) => {
    await page.locator('[data-testid="crm-tab-orgs"]').click();
    await page.locator('[data-testid="crm-new-org"]').click();
    const card = page.locator('[data-testid^="crm-org-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Organization")).toBeVisible();
  });

  test("pipeline tab renders stages", async ({ page }) => {
    await page.locator('[data-testid="crm-tab-pipeline"]').click();
    await expect(page.locator('[data-testid="crm-pipeline"]')).toBeVisible();
    await expect(page.locator("text=prospect")).toBeVisible();
    await expect(page.locator("text=qualified")).toBeVisible();
    await expect(page.locator("text=closed won")).toBeVisible();
  });
});

// ── Life Panel ──────────────────────────────────────────────────────────────

test.describe("Life Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-life"]').click();
    await expect(page.locator('[data-testid="life-panel"]')).toBeVisible({ timeout: 10_000 });
  });

  test("panel renders with all 6 tabs", async ({ page }) => {
    await expect(page.locator('[data-testid="life-tab-habits"]')).toBeVisible();
    await expect(page.locator('[data-testid="life-tab-fitness"]')).toBeVisible();
    await expect(page.locator('[data-testid="life-tab-sleep"]')).toBeVisible();
    await expect(page.locator('[data-testid="life-tab-journal"]')).toBeVisible();
    await expect(page.locator('[data-testid="life-tab-meals"]')).toBeVisible();
    await expect(page.locator('[data-testid="life-tab-cycle"]')).toBeVisible();
  });

  test("create and delete a habit", async ({ page }) => {
    await page.locator('[data-testid="life-new-habit"]').click();
    const card = page.locator('[data-testid^="life-habit-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Habit")).toBeVisible();
    await card.locator("text=Delete").click();
    await expect(card).not.toBeVisible({ timeout: 3000 });
  });

  test("log a workout", async ({ page }) => {
    await page.locator('[data-testid="life-tab-fitness"]').click();
    await page.locator('[data-testid="life-new-workout"]').click();
    const card = page.locator('[data-testid^="life-fitness-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=Workout")).toBeVisible();
  });

  test("log sleep", async ({ page }) => {
    await page.locator('[data-testid="life-tab-sleep"]').click();
    await page.locator('[data-testid="life-new-sleep"]').click();
    const card = page.locator('[data-testid^="life-sleep-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
  });

  test("create a journal entry", async ({ page }) => {
    await page.locator('[data-testid="life-tab-journal"]').click();
    await page.locator('[data-testid="life-new-journal"]').click();
    const card = page.locator('[data-testid^="life-journal-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=Journal Entry")).toBeVisible();
  });

  test("log a meal", async ({ page }) => {
    await page.locator('[data-testid="life-tab-meals"]').click();
    await page.locator('[data-testid="life-new-meal"]').click();
    const card = page.locator('[data-testid^="life-meal-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
  });

  test("create a cycle entry", async ({ page }) => {
    await page.locator('[data-testid="life-tab-cycle"]').click();
    await page.locator('[data-testid="life-new-cycle"]').click();
    const card = page.locator('[data-testid^="life-cycle-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=Cycle Entry")).toBeVisible();
  });
});

// ── Assets Management Panel ─────────────────────────────────────────────────

test.describe("Assets Management Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-assets-mgmt"]').click();
    await expect(page.locator('[data-testid="assets-mgmt-panel"]')).toBeVisible({ timeout: 10_000 });
  });

  test("panel renders with tabs", async ({ page }) => {
    await expect(page.locator('[data-testid="assets-tab-media"]')).toBeVisible();
    await expect(page.locator('[data-testid="assets-tab-content"]')).toBeVisible();
    await expect(page.locator('[data-testid="assets-tab-docs"]')).toBeVisible();
    await expect(page.locator('[data-testid="assets-tab-collections"]')).toBeVisible();
  });

  test("create and delete a media asset", async ({ page }) => {
    await page.locator('[data-testid="assets-new-media"]').click();
    const card = page.locator('[data-testid^="assets-media-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Media")).toBeVisible();
    await card.locator("text=Delete").click();
    await expect(card).not.toBeVisible({ timeout: 3000 });
  });

  test("create a content item", async ({ page }) => {
    await page.locator('[data-testid="assets-tab-content"]').click();
    await page.locator('[data-testid="assets-new-content"]').click();
    const card = page.locator('[data-testid^="assets-content-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Content")).toBeVisible();
  });

  test("scan a document", async ({ page }) => {
    await page.locator('[data-testid="assets-tab-docs"]').click();
    await page.locator('[data-testid="assets-new-doc"]').click();
    const card = page.locator('[data-testid^="assets-doc-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Scan")).toBeVisible();
  });

  test("create a collection", async ({ page }) => {
    await page.locator('[data-testid="assets-tab-collections"]').click();
    await page.locator('[data-testid="assets-new-collection"]').click();
    const card = page.locator('[data-testid^="assets-collection-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Collection")).toBeVisible();
  });
});

// ── Platform Panel ──────────────────────────────────────────────────────────

test.describe("Platform Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.locator('[data-testid="activity-icon-platform"]').click();
    await expect(page.locator('[data-testid="platform-panel"]')).toBeVisible({ timeout: 10_000 });
  });

  test("panel renders with tabs", async ({ page }) => {
    await expect(page.locator('[data-testid="platform-tab-calendar"]')).toBeVisible();
    await expect(page.locator('[data-testid="platform-tab-messages"]')).toBeVisible();
    await expect(page.locator('[data-testid="platform-tab-reminders"]')).toBeVisible();
    await expect(page.locator('[data-testid="platform-tab-feeds"]')).toBeVisible();
  });

  test("create and delete a calendar event", async ({ page }) => {
    await page.locator('[data-testid="platform-new-event"]').click();
    const card = page.locator('[data-testid^="platform-event-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Event")).toBeVisible();
    await card.locator("text=Delete").click();
    await expect(card).not.toBeVisible({ timeout: 3000 });
  });

  test("compose a message", async ({ page }) => {
    await page.locator('[data-testid="platform-tab-messages"]').click();
    await page.locator('[data-testid="platform-new-message"]').click();
    const card = page.locator('[data-testid^="platform-message-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Message")).toBeVisible();
  });

  test("create a reminder", async ({ page }) => {
    await page.locator('[data-testid="platform-tab-reminders"]').click();
    await page.locator('[data-testid="platform-new-reminder"]').click();
    const card = page.locator('[data-testid^="platform-reminder-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=Reminder")).toBeVisible();
  });

  test("add a feed", async ({ page }) => {
    await page.locator('[data-testid="platform-tab-feeds"]').click();
    await page.locator('[data-testid="platform-new-feed"]').click();
    const card = page.locator('[data-testid^="platform-feed-"]').first();
    await expect(card).toBeVisible({ timeout: 3000 });
    await expect(card.locator("text=New Feed")).toBeVisible();
  });
});
