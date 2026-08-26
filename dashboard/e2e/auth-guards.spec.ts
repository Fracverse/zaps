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
