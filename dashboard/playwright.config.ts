import { defineConfig, devices } from "@playwright/test";

/**
 * Playwright configuration for dashboard e2e tests.
 *
 * The suite drives a running Next.js dev server. Set PLAYWRIGHT_BASE_URL in CI
 * to point at an already-running deployment instead.
 *
 * Auth strategy: the dashboard guards on Privy, which has no CI-usable session,
 * so specs seed the browser's `privy:*` localStorage keys via
 * `page.addInitScript` before navigating. No live Privy or backend connection
 * is required.
 */
export default defineConfig({
  testDir: "./e2e",

  // Only *.spec.ts are tests. e2e/fixtures holds CSV files the disbursement
  // specs upload (#800); without this they sit inside testDir and any future
  // .ts helper added there would be collected as a suite.
  testMatch: /.*\.spec\.ts$/,

  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? "github" : "list",

  // The dev server compiles a route on first request, and /dashboard/payouts is
  // a large client component. The default 30s can expire on that first compile
  // before the page has rendered anything to assert against.
  timeout: 60_000,
  expect: { timeout: 15_000 },

  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL ?? "http://localhost:3000",
    trace: "on-first-retry",
    // Each spec seeds its own auth state, so no shared storage state is loaded.
    storageState: undefined,
  },

  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
    {
      // #808 — the mobile drawer only exists below the md breakpoint, so it
      // needs a viewport narrower than 768px to be exercised at all.
      name: "mobile-chrome",
      use: { ...devices["Pixel 7"] },
      testMatch: /mobile-nav\.spec\.ts$/,
    },
  ],

  /* Start the Next.js dev server when running locally */
  webServer: process.env.CI
    ? undefined
    : {
        command: "npm run dev",
        url: "http://localhost:3000",
        reuseExistingServer: true,
        timeout: 120_000,
      },
});
