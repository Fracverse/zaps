/**
 * dashboard/e2e/mobile-nav.spec.ts
 *
 * Responsive dashboard navigation (#808).
 *
 * Runs under the `mobile-chrome` project, which uses a viewport below the
 * 768px breakpoint — the drawer does not exist above it, so a desktop viewport
 * would pass these vacuously.
 */

import { test, expect, type Page } from "@playwright/test";

/** Seed a Privy session and stub the dashboard's data calls. */
async function openDashboard(page: Page, route = "/dashboard"): Promise<void> {
  await page.addInitScript(() => {
    const expiry = Date.now() + 60 * 60 * 1000;
    window.localStorage.setItem("privy:token", "e2e-mock-access-token");
    window.localStorage.setItem(
      "privy:session",
      JSON.stringify({ expiry, userId: "did:privy:e2e-user" }),
    );
    window.localStorage.setItem("token", "e2e-mock-api-token");
  });

  await page.route("**/api/**", (route_) =>
    route_.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ data: [], batches: [], total: 0 }),
    }),
  );

  await page.goto(route);
}

const trigger = (page: Page) => page.getByTestId("mobile-nav-trigger");
const drawer = (page: Page) => page.getByTestId("mobile-nav-drawer");

test.describe("Mobile navigation drawer", () => {
  test.beforeEach(async ({ page }) => {
    await openDashboard(page);
    await expect(trigger(page)).toBeVisible({ timeout: 20_000 });
  });

  test("hides the fixed sidebar on a narrow viewport", async ({ page }) => {
    // The element may still be in the DOM; what matters is that it is not
    // painted over the content.
    await expect(page.getByTestId("desktop-sidebar")).toBeHidden();
  });

  test("opens the drawer from the hamburger", async ({ page }) => {
    await expect(drawer(page)).toHaveCount(0);

    await trigger(page).click();

    await expect(drawer(page)).toBeVisible();
    await expect(
      page.getByRole("dialog", { name: /dashboard navigation/i }),
    ).toBeVisible();
  });

  test("lists every dashboard route in the drawer", async ({ page }) => {
    await trigger(page).click();
    const panel = page.getByRole("dialog", { name: /dashboard navigation/i });

    for (const label of [
      "Overview",
      "Transactions",
      "Payouts",
      "QR Codes",
      "Analytics",
      "Contracts",
      "Yield Vault",
    ]) {
      await expect(panel.getByRole("link", { name: label })).toBeVisible();
    }
  });

  test("closes on the close button", async ({ page }) => {
    await trigger(page).click();
    await expect(drawer(page)).toBeVisible();

    await page.getByRole("button", { name: /close navigation menu/i }).click();

    await expect(drawer(page)).toHaveCount(0);
  });

  test("closes when the scrim is tapped", async ({ page }) => {
    await trigger(page).click();
    await expect(drawer(page)).toBeVisible();

    await page.getByTestId("mobile-nav-scrim").click({ position: { x: 5, y: 5 } });

    await expect(drawer(page)).toHaveCount(0);
  });

  test("closes on Escape", async ({ page }) => {
    await trigger(page).click();
    await expect(drawer(page)).toBeVisible();

    await page.keyboard.press("Escape");

    await expect(drawer(page)).toHaveCount(0);
  });

  test("navigates and dismisses itself, rather than covering the destination", async ({
    page,
  }) => {
    await trigger(page).click();
    await page
      .getByRole("dialog", { name: /dashboard navigation/i })
      .getByRole("link", { name: "Payouts" })
      .click();

    await page.waitForURL("**/dashboard/payouts", { timeout: 20_000 });
    await expect(drawer(page)).toHaveCount(0);
  });

  test("marks the current route for assistive technology", async ({ page }) => {
    await openDashboard(page, "/dashboard/analytics");
    await trigger(page).click();

    await expect(
      page.getByRole("dialog").getByRole("link", { name: "Analytics" }),
    ).toHaveAttribute("aria-current", "page");
  });

  test("returns focus to the hamburger when dismissed", async ({ page }) => {
    await trigger(page).click();
    await expect(drawer(page)).toBeVisible();

    await page.keyboard.press("Escape");
    await expect(drawer(page)).toHaveCount(0);

    await expect(trigger(page)).toBeFocused();
  });

  test("locks the page behind the open drawer from scrolling", async ({
    page,
  }) => {
    await trigger(page).click();
    await expect(drawer(page)).toBeVisible();

    await expect(async () => {
      const overflow = await page.evaluate(() => document.body.style.overflow);
      expect(overflow).toBe("hidden");
    }).toPass();

    await page.keyboard.press("Escape");
    await expect(drawer(page)).toHaveCount(0);

    const restored = await page.evaluate(() => document.body.style.overflow);
    expect(restored).not.toBe("hidden");
  });
});

test.describe("Desktop navigation is unaffected", () => {
  test.use({ viewport: { width: 1280, height: 800 } });

  test("shows the sidebar and hides the hamburger above the breakpoint", async ({
    page,
  }) => {
    await openDashboard(page);

    await expect(page.getByTestId("desktop-sidebar")).toBeVisible({
      timeout: 20_000,
    });
    await expect(page.getByTestId("mobile-nav-trigger")).toBeHidden();
  });
});
