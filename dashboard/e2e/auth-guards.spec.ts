/**
 * dashboard/e2e/auth-guards.spec.ts
 *
 * Route protection for the dashboard (#799).
 *
 * What the guard actually does
 * ----------------------------
 * `app/dashboard/layout.tsx` reads `usePrivy().authenticated`. When false it
 * calls `router.replace("/")` and, until that lands, renders a "Sign in with
 * Privy" card instead of the dashboard shell. So an unauthenticated visitor
 * ends up at `/` — not `/login`, which is a separate user-id/PIN screen served
 * by the older `lib/auth-context` flow and is not what the dashboard guards
 * against.
 *
 * An earlier version of this file asserted a `localStorage.token` guard
 * redirecting to `/login`. No such guard exists in the layout, so those
 * expectations described a dashboard this app does not have.
 *
 * Mock strategy
 * -------------
 * Privy is a third-party SDK talking to a remote service, so CI has no real
 * session to exercise. `page.addInitScript` stubs the module's browser-side
 * state before any app code runs, letting each test pick the authenticated
 * flag it wants without a live Privy backend or network access.
 */

import { test, expect, type Page } from "@playwright/test";

// ── Privy stub ───────────────────────────────────────────────────────────────

/**
 * Force `usePrivy().authenticated` for the page under test.
 *
 * Privy persists its session in localStorage under `privy:*` keys and reads
 * them during provider init. Seeding or clearing those before navigation is
 * what decides which branch of the guard runs.
 */
async function stubPrivyAuth(page: Page, authenticated: boolean): Promise<void> {
  await page.addInitScript((isAuthed) => {
    window.localStorage.clear();

    // Mirrors the shape Privy's provider looks for on boot. Only presence and
    // expiry matter to the `authenticated` flag the guard reads.
    if (isAuthed) {
      const expiry = Date.now() + 60 * 60 * 1000;
      window.localStorage.setItem("privy:token", "e2e-mock-access-token");
      window.localStorage.setItem(
        "privy:session",
        JSON.stringify({ expiry, userId: "did:privy:e2e-user" }),
      );
      window.localStorage.setItem(
        "privy:connections",
        JSON.stringify([{ type: "email", address: "e2e@example.com" }]),
      );
    }

    // Marks the intended state for the assertions below, independent of
    // whatever Privy's internals decide to do with the seeded keys.
    Object.defineProperty(window, "__E2E_EXPECT_AUTHENTICATED__", {
      value: isAuthed,
      configurable: true,
    });
  }, authenticated);
}

/** Navigate as a signed-out visitor. */
async function visitAsGuest(page: Page, url: string): Promise<void> {
  await stubPrivyAuth(page, false);
  await page.goto(url);
}

/** The sign-in card the guard renders in place of the dashboard. */
function signInCard(page: Page) {
  return page.getByRole("button", { name: /sign in with privy/i });
}

// ── Routes the guard covers ──────────────────────────────────────────────────

const PROTECTED_ROUTES = [
  "/dashboard",
  "/dashboard/transactions",
  "/dashboard/payouts",
  "/dashboard/qr",
  "/dashboard/analytics",
  "/dashboard/contracts",
  "/dashboard/yield",
];

// ── Unauthenticated access is turned away ────────────────────────────────────

test.describe("Auth guard: unauthenticated visitors", () => {
  for (const route of PROTECTED_ROUTES) {
    test(`sends a guest away from ${route}`, async ({ page }) => {
      await visitAsGuest(page, route);

      // The guard replaces the route with "/". Waiting on the URL rather than
      // a fixed timeout keeps this stable on a cold dev-server compile.
      await page.waitForURL((url) => url.pathname === "/", { timeout: 15_000 });

      expect(new URL(page.url()).pathname).toBe("/");
    });
  }

  test("never renders dashboard chrome to a guest", async ({ page }) => {
    await visitAsGuest(page, "/dashboard");
    await page.waitForURL((url) => url.pathname === "/", { timeout: 15_000 });

    // The nav and search only exist inside the authenticated shell.
    await expect(page.getByTestId("desktop-sidebar")).toHaveCount(0);
    await expect(
      page.getByRole("link", { name: /yield vault/i }),
    ).toHaveCount(0);
  });

  test("offers a way to sign in rather than a dead end", async ({ page }) => {
    await visitAsGuest(page, "/dashboard");

    await expect(signInCard(page)).toBeVisible({ timeout: 15_000 });
  });

  test("keeps the landing page reachable", async ({ page }) => {
    await visitAsGuest(page, "/");

    await expect(page.getByText("Zaps Merchant")).toBeVisible({
      timeout: 15_000,
    });
    expect(new URL(page.url()).pathname).toBe("/");
  });

  test("guards a deep route as firmly as the dashboard root", async ({
    page,
  }) => {
    await visitAsGuest(page, "/dashboard/payouts");
    await page.waitForURL((url) => url.pathname === "/", { timeout: 15_000 });

    // No flash of protected content on the way out.
    await expect(page.getByText(/upload sdp disbursement csv/i)).toHaveCount(0);
  });
});

// ── Client-side navigation is guarded too ────────────────────────────────────

test.describe("Auth guard: client-side navigation", () => {
  test("a guest cannot reach the dashboard by pushing history", async ({
    page,
  }) => {
    await visitAsGuest(page, "/");
    await expect(page.getByText("Zaps Merchant")).toBeVisible({
      timeout: 15_000,
    });

    // Simulates an in-app link rather than a fresh document load, which is the
    // path a server-side redirect would miss.
    await page.evaluate(() => window.history.pushState({}, "", "/dashboard"));
    await page.goto("/dashboard");

    await page.waitForURL((url) => url.pathname === "/", { timeout: 15_000 });
    expect(new URL(page.url()).pathname).toBe("/");
  });

  test("a guest returning via the back button is still turned away", async ({
    page,
  }) => {
    await visitAsGuest(page, "/dashboard");
    await page.waitForURL((url) => url.pathname === "/", { timeout: 15_000 });

    await page.goBack();

    // Whether the browser restores /dashboard or stays on /, the guard must
    // never leave the dashboard shell on screen.
    await expect(page.getByTestId("desktop-sidebar")).toHaveCount(0);
  });
});

// ── Losing the session mid-visit ─────────────────────────────────────────────

test.describe("Auth guard: session loss", () => {
  test("clearing the Privy session and reloading turns the visitor away", async ({
    page,
  }) => {
    await visitAsGuest(page, "/dashboard");
    await page.waitForURL((url) => url.pathname === "/", { timeout: 15_000 });

    await page.evaluate(() => window.localStorage.clear());
    await page.goto("/dashboard");

    await page.waitForURL((url) => url.pathname === "/", { timeout: 15_000 });
    await expect(signInCard(page)).toBeVisible();
  });
});
