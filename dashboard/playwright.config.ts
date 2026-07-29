import { defineConfig, devices } from "@playwright/test";

/**
 * Playwright configuration for dashboard e2e tests.
 *
 * Auth guard tests rely on a running Next.js dev server. Set PLAYWRIGHT_BASE_URL
 * in CI to override the default local URL.
 *
 * Pipeline auth override: tests mock localStorage `token` directly to simulate
 * authenticated / unauthenticated states without a live Privy backend.
 */
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? "github" : "list",

  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL ?? "http://localhost:3000",
    trace: "on-first-retry",
    // Ensure localStorage manipulation is possible before navigation
    storageState: undefined,
  },

  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
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
