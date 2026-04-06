import { test, expect } from "@playwright/test";

test.describe("Relay Manager Panel", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    // Open the Relay lens via activity bar
    await page.locator('[data-testid="activity-icon-relay"]').click();
    await expect(page.locator('[data-testid="tab-relay"]')).toBeVisible();
  });

  // ── Panel rendering ───────────────────────────────────────────────────

  test("relay panel renders with header and sections", async ({ page }) => {
    await expect(page.locator("text=Relay Manager")).toBeVisible();
    await expect(page.getByRole("button", { name: "Add Relay" })).toBeVisible();
    await expect(page.getByText("Relays", { exact: true })).toBeVisible();
    await expect(page.locator("text=CLI Reference")).toBeVisible();
  });

  test("shows empty state when no relays configured", async ({ page }) => {
    await expect(
      page.locator("text=No relays configured"),
    ).toBeVisible();
  });

  test("shows CLI reference commands", async ({ page }) => {
    await expect(page.locator("text=prism-relay --mode dev --port 4444")).toBeVisible();
    await expect(page.locator("text=prism-relay --mode server --port 443")).toBeVisible();
    await expect(page.locator("text=prism-relay --mode p2p --port 8080")).toBeVisible();
  });

  // ── Add relay form ────────────────────────────────────────────────────

  test("add relay form has name and URL inputs", async ({ page }) => {
    const nameInput = page.locator('input[placeholder="Name"]');
    const urlInput = page.locator('input[placeholder="https://relay.example.com"]');
    await expect(nameInput).toBeVisible();
    await expect(urlInput).toBeVisible();
  });

  test("adding a relay shows it in the list", async ({ page }) => {
    await page.locator('input[placeholder="Name"]').fill("My Relay");
    await page.locator('input[placeholder="https://relay.example.com"]').fill("http://localhost:4444");
    await page.locator("button", { hasText: "Add Relay" }).click();

    // Relay card should appear
    await expect(page.locator("text=My Relay").first()).toBeVisible();
    await expect(page.getByText("http://localhost:4444", { exact: true })).toBeVisible();

    // Status should be disconnected
    await expect(page.locator("text=disconnected").first()).toBeVisible();

    // Empty state should be gone
    await expect(page.locator("text=No relays configured")).not.toBeVisible();
  });

  test("adding relay shows notification", async ({ page }) => {
    await page.locator('input[placeholder="Name"]').fill("Test Relay");
    await page.locator('input[placeholder="https://relay.example.com"]').fill("http://localhost:9999");
    await page.locator("button", { hasText: "Add Relay" }).click();

    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(toast.first()).toBeVisible({ timeout: 2000 });
    await expect(toast.first()).toContainText("Added relay");
  });

  test("adding relay clears form inputs", async ({ page }) => {
    const nameInput = page.locator('input[placeholder="Name"]');
    const urlInput = page.locator('input[placeholder="https://relay.example.com"]');

    await nameInput.fill("Temp");
    await urlInput.fill("http://localhost:1234");
    await page.locator("button", { hasText: "Add Relay" }).click();

    await expect(nameInput).toHaveValue("");
    await expect(urlInput).toHaveValue("");
  });

  test("pressing Enter in URL input adds relay", async ({ page }) => {
    await page.locator('input[placeholder="Name"]').fill("Enter Relay");
    const urlInput = page.locator('input[placeholder="https://relay.example.com"]');
    await urlInput.fill("http://localhost:5555");
    await urlInput.press("Enter");

    await expect(page.locator("text=Enter Relay").first()).toBeVisible();
  });

  // ── Relay card actions ────────────────────────────────────────────────

  test("removing a relay removes it from the list", async ({ page }) => {
    // Add a relay
    await page.locator('input[placeholder="Name"]').fill("Doomed");
    await page.locator('input[placeholder="https://relay.example.com"]').fill("http://localhost:6666");
    await page.locator("button", { hasText: "Add Relay" }).click();
    await expect(page.locator("text=Doomed").first()).toBeVisible();

    // Remove it
    await page.locator("button", { hasText: "Remove" }).click();

    // Should show empty state again
    await expect(page.locator("text=No relays configured")).toBeVisible();
  });

  test("removing relay shows notification", async ({ page }) => {
    await page.locator('input[placeholder="Name"]').fill("Bye Relay");
    await page.locator('input[placeholder="https://relay.example.com"]').fill("http://localhost:7777");
    await page.locator("button", { hasText: "Add Relay" }).click();

    // Wait for add notification to appear and then dismiss
    await expect(page.locator('[data-testid="notification-toast"]').first()).toBeVisible({ timeout: 2000 });

    await page.locator("button", { hasText: "Remove" }).click();
    // Check for removal notification
    await expect(
      page.locator('[data-testid="notification-toast"]').filter({ hasText: "Removed relay" }),
    ).toBeVisible({ timeout: 2000 });
  });

  test("disconnected relay shows Connect button", async ({ page }) => {
    await page.locator('input[placeholder="Name"]').fill("TestRelay");
    await page.locator('input[placeholder="https://relay.example.com"]').fill("http://localhost:4444");
    await page.locator("button", { hasText: "Add Relay" }).click();

    // Should have a Connect button
    await expect(
      page.locator("button", { hasText: "Connect" }),
    ).toBeVisible();
  });

  test("connect button shows CLI guidance notification", async ({ page }) => {
    await page.locator('input[placeholder="Name"]').fill("TestRelay");
    await page.locator('input[placeholder="https://relay.example.com"]').fill("http://localhost:4444");
    await page.locator("button", { hasText: "Add Relay" }).click();

    // Click connect
    await page.locator("button", { hasText: "Connect" }).click();

    // Should show CLI guidance
    const toast = page.locator('[data-testid="notification-toast"]');
    await expect(
      toast.filter({ hasText: "Connect via CLI" }),
    ).toBeVisible({ timeout: 2000 });
  });

  // ── Multiple relays ───────────────────────────────────────────────────

  test("multiple relays can be added", async ({ page }) => {
    // Add first relay
    await page.locator('input[placeholder="Name"]').fill("Relay A");
    await page.locator('input[placeholder="https://relay.example.com"]').fill("http://a.example.com");
    await page.locator("button", { hasText: "Add Relay" }).click();

    // Add second relay
    await page.locator('input[placeholder="Name"]').fill("Relay B");
    await page.locator('input[placeholder="https://relay.example.com"]').fill("http://b.example.com");
    await page.locator("button", { hasText: "Add Relay" }).click();

    // Both should be visible
    await expect(page.locator("text=Relay A").first()).toBeVisible();
    await expect(page.locator("text=Relay B").first()).toBeVisible();

    // Summary should show 2 relays
    await expect(page.locator("text=2 relays configured")).toBeVisible();
  });

  test("removing one relay keeps the other", async ({ page }) => {
    // Add two relays
    await page.locator('input[placeholder="Name"]').fill("Keep");
    await page.locator('input[placeholder="https://relay.example.com"]').fill("http://keep.example.com");
    await page.locator("button", { hasText: "Add Relay" }).click();

    await page.locator('input[placeholder="Name"]').fill("Remove");
    await page.locator('input[placeholder="https://relay.example.com"]').fill("http://remove.example.com");
    await page.locator("button", { hasText: "Add Relay" }).click();

    // Remove the second one (last "Remove" button)
    const removeButtons = page.locator("button", { hasText: "Remove" });
    await removeButtons.last().click();

    // "Keep" should still be there
    await expect(page.locator("text=Keep").first()).toBeVisible();
    // "Remove" relay text should be gone
    await expect(page.locator("text=http://remove.example.com")).not.toBeVisible();
  });

  // ── KBar navigation ───────────────────────────────────────────────────

  test("KBar can navigate to Relay panel", async ({ page }) => {
    // Close relay tab first
    await page.locator('[data-testid="tab-close-relay"]').click();
    await expect(page.locator("text=Relay Manager")).not.toBeVisible();

    // Use KBar to open relay
    const isMac = await page.evaluate(() =>
      /Mac|iPod|iPhone|iPad/.test(navigator.platform),
    );
    await page.keyboard.press(isMac ? "Meta+k" : "Control+k");
    const kbarInput = page.locator('[role="combobox"]');
    await expect(kbarInput).toBeVisible({ timeout: 3_000 });

    await kbarInput.type("Relay", { delay: 50 });
    await page.locator("text=Switch to Relay").first().click();

    await expect(page.locator("text=Relay Manager")).toBeVisible({ timeout: 5_000 });
  });

  // ── Summary counter ───────────────────────────────────────────────────

  test("summary counter updates as relays are added/removed", async ({ page }) => {
    // Initially 0
    await expect(page.locator("text=0 relays configured")).toBeVisible();

    // Add one
    await page.locator('input[placeholder="Name"]').fill("R1");
    await page.locator('input[placeholder="https://relay.example.com"]').fill("http://r1.example.com");
    await page.locator("button", { hasText: "Add Relay" }).click();
    await expect(page.locator("text=1 relay configured")).toBeVisible();

    // Add another
    await page.locator('input[placeholder="Name"]').fill("R2");
    await page.locator('input[placeholder="https://relay.example.com"]').fill("http://r2.example.com");
    await page.locator("button", { hasText: "Add Relay" }).click();
    await expect(page.locator("text=2 relays configured")).toBeVisible();

    // Remove one
    await page.locator("button", { hasText: "Remove" }).first().click();
    await expect(page.locator("text=1 relay configured")).toBeVisible();
  });
});
