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

// ── Yield Vault: parameter updates (#804) ────────────────────────────────────

/**
 * Freighter is a browser extension, so Playwright has no way to install or
 * drive the real wallet. `lib/freighter.ts` resolves its API through
 * `loadFreighterApi()`, which prefers `window.__zapsFreighterApiMock__` outside
 * production builds — these tests install a stand-in there before the app
 * boots. The seam is compiled out of production, so a real user's page cannot
 * be pointed at a fake wallet.
 */
const FREIGHTER_MOCK_KEY = "__zapsFreighterApiMock__";
const MOCK_PUBLIC_KEY = "GD3XABCDEFGHIJKLMNOPQRSTUVWXYZ12345678ABCD";
const MOCK_SIGNED_XDR = "AAAAAgAAAABsignedEnvelopeFromMockWallet==";

interface WalletMockOptions {
  /** Simulate the extension not being installed at all. */
  installed?: boolean;
  /** Simulate the extension present but not yet granted access. */
  connected?: boolean;
  /** Make signTransaction resolve with an error, as a user rejection does. */
  signError?: string;
}

/**
 * Install a fake Freighter and a mock auth token, then open the vault page.
 */
async function visitVaultWithWallet(
  page: Page,
  options: WalletMockOptions = {},
): Promise<void> {
  const { installed = true, connected = true, signError } = options;

  await page.addInitScript(
    ({ key, publicKey, signedXdr, installed, connected, signError }) => {
      window.localStorage.setItem("token", "mock-auth-token-for-e2e");

      if (!installed) {
        // detectFreighter() treats a throwing API as "extension not present".
        (window as unknown as Record<string, unknown>)[key] = {
          isConnected: () => Promise.reject(new Error("not installed")),
          getAddress: () => Promise.reject(new Error("not installed")),
          getNetwork: () => Promise.reject(new Error("not installed")),
          requestAccess: () => Promise.reject(new Error("not installed")),
          signTransaction: () => Promise.reject(new Error("not installed")),
        };
        return;
      }

      const calls: unknown[] = [];
      (window as unknown as Record<string, unknown>).__freighterSignCalls = calls;

      (window as unknown as Record<string, unknown>)[key] = {
        isConnected: () => Promise.resolve({ isConnected: connected }),
        getAddress: () => Promise.resolve({ address: publicKey }),
        getNetwork: () => Promise.resolve({ network: "TESTNET" }),
        requestAccess: () => Promise.resolve({ address: publicKey }),
        signTransaction: (xdr: string, opts: { networkPassphrase: string }) => {
          calls.push({ xdr, opts });
          return signError
            ? Promise.resolve({ error: signError })
            : Promise.resolve({ signedTxXdr: signedXdr });
        },
      };
    },
    {
      key: FREIGHTER_MOCK_KEY,
      publicKey: MOCK_PUBLIC_KEY,
      signedXdr: MOCK_SIGNED_XDR,
      installed,
      connected,
      signError,
    },
  );

  await page.goto("/dashboard/yield");
  await page.waitForLoadState("networkidle");
}

/** Fill the APY field, tick the confirmation box, and submit. */
async function submitApy(page: Page, apy: string): Promise<void> {
  const apyInput = page.getByLabel("APY (%)");
  await apyInput.fill(apy);
  await page.locator("#vault-confirm").check();
  await page.getByRole("button", { name: /sign & submit via freighter/i }).click();
}

test.describe("Yield Vault: APY parameter updates", () => {
  test("admin can update APY and sign with Freighter", async ({ page }) => {
    await visitVaultWithWallet(page);

    await submitApy(page, "7.5");

    await expect(page.getByTestId("vault-success")).toBeVisible({ timeout: 8_000 });
    await expect(page.getByTestId("vault-success")).toContainText(
      /transaction signed and submitted/i,
    );
  });

  test("the APY value entered is what gets signed", async ({ page }) => {
    await visitVaultWithWallet(page);

    await submitApy(page, "12.25");
    await expect(page.getByTestId("vault-success")).toBeVisible({ timeout: 8_000 });

    // The page encodes the vault params as base64 JSON before handing them to
    // the wallet, so decoding the captured XDR proves the form value survived.
    const signed = await page.evaluate(() => {
      const calls = (window as unknown as Record<string, unknown>).__freighterSignCalls as
        | { xdr: string; opts: { networkPassphrase: string } }[]
        | undefined;
      if (!calls?.length) return null;
      const last = calls[calls.length - 1];
      return { payload: JSON.parse(atob(last.xdr)), passphrase: last.opts.networkPassphrase };
    });

    expect(signed).not.toBeNull();
    expect(signed!.payload.fn).toBe("set_vault_params");
    expect(signed!.payload.apy).toBe("12.25");
    expect(signed!.passphrase).toBeTruthy();
  });

  test("the connected wallet address is submitted as the admin", async ({ page }) => {
    await visitVaultWithWallet(page);

    await submitApy(page, "6.0");
    await expect(page.getByTestId("vault-success")).toBeVisible({ timeout: 8_000 });

    const payload = await page.evaluate(() => {
      const calls = (window as unknown as Record<string, unknown>).__freighterSignCalls as
        | { xdr: string }[]
        | undefined;
      return calls?.length ? JSON.parse(atob(calls[calls.length - 1].xdr)) : null;
    });

    expect(payload.admin).toBe(MOCK_PUBLIC_KEY);
  });

  test("the pause flag is carried through with the APY change", async ({ page }) => {
    await visitVaultWithWallet(page);

    await page.getByRole("switch", { name: /pause vault/i }).click();
    await submitApy(page, "5.5");
    await expect(page.getByTestId("vault-success")).toBeVisible({ timeout: 8_000 });

    const payload = await page.evaluate(() => {
      const calls = (window as unknown as Record<string, unknown>).__freighterSignCalls as
        | { xdr: string }[]
        | undefined;
      return calls?.length ? JSON.parse(atob(calls[calls.length - 1].xdr)) : null;
    });

    expect(payload.paused).toBe(true);
    expect(payload.apy).toBe("5.5");
  });

  test("submission is blocked until the confirmation box is ticked", async ({ page }) => {
    await visitVaultWithWallet(page);

    await page.getByLabel("APY (%)").fill("9.0");

    const submit = page.getByRole("button", { name: /sign & submit via freighter/i });
    await expect(submit).toBeDisabled();

    await page.locator("#vault-confirm").check();
    await expect(submit).toBeEnabled();
  });

  test("the confirmation box is cleared after a successful submission", async ({ page }) => {
    await visitVaultWithWallet(page);

    await submitApy(page, "8.0");
    await expect(page.getByTestId("vault-success")).toBeVisible({ timeout: 8_000 });

    // Re-arming for every change is deliberate: an admin should have to
    // confirm each on-chain write, not just the first.
    await expect(page.locator("#vault-confirm")).not.toBeChecked();
  });

  test("a rejected signature surfaces the wallet's message, not a success toast", async ({
    page,
  }) => {
    await visitVaultWithWallet(page, { signError: "User declined access" });

    await submitApy(page, "7.5");

    await expect(page.getByTestId("vault-error")).toBeVisible({ timeout: 8_000 });
    await expect(page.getByTestId("vault-error")).toContainText(/user declined access/i);
    await expect(page.getByTestId("vault-success")).toHaveCount(0);
  });

  test("submission is unavailable while the wallet is disconnected", async ({ page }) => {
    await visitVaultWithWallet(page, { connected: false });

    const submit = page.getByRole("button", { name: /connect wallet to sign/i });
    await expect(submit).toBeVisible();
    await expect(submit).toBeDisabled();
  });
});
