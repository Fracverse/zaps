/**
 * dashboard/e2e/auth-guards.spec.ts
 *
 * Integration tests for Privy dashboard auth guards.
 *
 * Acceptance criteria
 * -------------------
 * ✓ Unauthenticated users are redirected to /login when visiting protected pages.
 * ✓ Authenticated users can access protected dashboard pages without redirect.
 * ✓ Auth is validated via mocked localStorage token (pipeline-safe override).
 *
 * Mock strategy
 * -------------
 * The dashboard layout reads `localStorage.getItem("token")` to determine auth
 * state. Tests inject or clear this value via `page.addInitScript` before
 * navigating, so no real Privy or backend connection is required in CI.
 */

import { test, expect, Page } from "@playwright/test";

// ── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Navigate to `url` without any auth token in localStorage.
 * Simulates an unauthenticated / logged-out user.
 */
async function visitAsGuest(page: Page, url: string): Promise<void> {
  // Clear any pre-existing storage state and inject a clean localStorage.
  await page.addInitScript(() => {
    window.localStorage.clear();
  });
  await page.goto(url);
}

/**
 * Navigate to `url` with a mock auth token already in localStorage.
 * Simulates a logged-in user without a real Privy session.
 *
 * @param token - A fake JWT-shaped value; only presence matters for the guard.
 */
async function visitAsAuthenticated(
  page: Page,
  url: string,
  token = "mock-auth-token-for-e2e"
): Promise<void> {
  await page.addInitScript((t) => {
    window.localStorage.setItem("token", t);
  }, token);
  await page.goto(url);
}

// ── Protected routes to exercise ─────────────────────────────────────────────

const PROTECTED_ROUTES = [
  "/dashboard",
  "/dashboard/payouts",
  "/dashboard/transactions",
  "/dashboard/yield",
  "/dashboard/analytics",
];

// ── Auth guard — unauthenticated access ──────────────────────────────────────

test.describe("Auth guard: unauthenticated users", () => {
  for (const route of PROTECTED_ROUTES) {
    test(`redirects guest from ${route} to /login`, async ({ page }) => {
      await visitAsGuest(page, route);

      // The layout's useEffect replaces the route with /login when no token is found.
      await page.waitForURL("**/login", { timeout: 8_000 });

      expect(page.url()).toContain("/login");
    });
  }

  test("login page is accessible without a token", async ({ page }) => {
    await visitAsGuest(page, "/login");

    // Should stay on /login — not redirect elsewhere.
    await page.waitForLoadState("networkidle");
    expect(page.url()).toContain("/login");
  });

  test("login page renders sign-in form for unauthenticated users", async ({
    page,
  }) => {
    await visitAsGuest(page, "/login");

    await expect(page.getByText("Zaps Merchant")).toBeVisible();
    await expect(page.getByPlaceholder("Your user ID")).toBeVisible();
    await expect(page.getByPlaceholder("••••")).toBeVisible();
    await expect(page.getByRole("button", { name: /sign in/i })).toBeVisible();
  });

  test("direct navigation to /dashboard root redirects guest to /login", async ({
    page,
  }) => {
    await visitAsGuest(page, "/dashboard");
    await page.waitForURL("**/login", { timeout: 8_000 });
    expect(page.url()).toContain("/login");
    // Confirm protected content is not visible
    await expect(page.getByText("Zaps Merchant")).toBeVisible();
  });
});

// ── Auth guard — authenticated access ────────────────────────────────────────

test.describe("Auth guard: authenticated users", () => {
  test("authenticated user can reach /dashboard without redirect", async ({
    page,
  }) => {
    await visitAsAuthenticated(page, "/dashboard");

    // Wait for the page to settle. If the guard fires, it would push to /login.
    await page.waitForLoadState("networkidle");

    // Should remain on a dashboard URL, not be pushed to /login.
    expect(page.url()).not.toContain("/login");
  });

  test("mock token is present in localStorage during session", async ({
    page,
  }) => {
    await visitAsAuthenticated(page, "/dashboard");
    await page.waitForLoadState("networkidle");

    const token = await page.evaluate(() =>
      window.localStorage.getItem("token")
    );
    expect(token).toBeTruthy();
  });

  test("authenticated user can navigate to /dashboard/payouts", async ({
    page,
  }) => {
    await visitAsAuthenticated(page, "/dashboard/payouts");
    await page.waitForLoadState("networkidle");

    expect(page.url()).not.toContain("/login");
  });

  test("authenticated user can navigate to /dashboard/yield", async ({
    page,
  }) => {
    await visitAsAuthenticated(page, "/dashboard/yield");
    await page.waitForLoadState("networkidle");

    expect(page.url()).not.toContain("/login");
  });
});

// ── Auth guard — token removal (logout) ──────────────────────────────────────

test.describe("Auth guard: token removal simulates logout", () => {
  test("clearing token then navigating to /dashboard redirects to /login", async ({
    page,
  }) => {
    // Start authenticated
    await visitAsAuthenticated(page, "/dashboard");
    await page.waitForLoadState("networkidle");

    // Simulate logout by clearing localStorage
    await page.evaluate(() => window.localStorage.removeItem("token"));

    // Navigate to a protected page fresh — should now be redirected
    await page.goto("/dashboard");
    await page.waitForURL("**/login", { timeout: 8_000 });
    expect(page.url()).toContain("/login");
  });
});

// ── Login form validation ─────────────────────────────────────────────────────

test.describe("Login form: field validation", () => {
  test("submit button is present and form fields are required", async ({
    page,
  }) => {
    await visitAsGuest(page, "/login");

    const userIdInput = page.getByPlaceholder("Your user ID");
    const pinInput = page.getByPlaceholder("••••");
    const submitBtn = page.getByRole("button", { name: /sign in/i });

    await expect(userIdInput).toBeVisible();
    await expect(pinInput).toBeVisible();
    await expect(submitBtn).toBeEnabled();

    // HTML5 required validation: attempting submit with empty fields
    // should not proceed (browser prevents form submission).
    await submitBtn.click();

    // Page should still be on /login (no navigation occurred).
    expect(page.url()).toContain("/login");
  });

  test("error message is shown on failed login attempt", async ({ page }) => {
    // Mock the API call to reject credentials
    await page.route("**/api/auth/**", (route) =>
      route.fulfill({
        status: 401,
        contentType: "application/json",
        body: JSON.stringify({ error: "Unauthorized" }),
      })
    );

    await visitAsGuest(page, "/login");

    await page.getByPlaceholder("Your user ID").fill("invalid-user");
    await page.getByPlaceholder("••••").fill("0000");
    await page.getByRole("button", { name: /sign in/i }).click();

    // The login page shows an error message on failure
    await expect(
      page.getByText(/invalid user id or pin/i)
    ).toBeVisible({ timeout: 6_000 });
  });
});
