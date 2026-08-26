/**
 * dashboard/e2e/csv-disbursement.spec.ts
 *
 * CSV batch disbursement upload and validation (#800).
 *
 * Covers the SDP CSV tab on /dashboard/payouts: uploading a file through the
 * react-dropzone input, and the validation summary that has to appear before a
 * merchant is allowed to send money. The row counts are the point — a preview
 * that silently drops or miscounts recipients is how the wrong number of
 * people get paid.
 *
 * The file input is fed with `page.setInputFiles`, which works on the hidden
 * input react-dropzone renders and does not depend on drag-and-drop emulation.
 */

import path from "node:path";
import { test, expect, type Page } from "@playwright/test";

const FIXTURES = path.join(__dirname, "fixtures");

/** Rows in `disbursement-valid.csv`. */
const VALID_ROW_COUNT = 5;

/** `disbursement-mixed.csv` carries 2 good rows and 2 bad ones. */
const MIXED_VALID = 2;
const MIXED_INVALID = 2;

// ── Setup ────────────────────────────────────────────────────────────────────

/**
 * Put the browser in an authenticated state and stub the payout APIs.
 *
 * The dashboard layout guards on Privy, and the payouts page fetches batch and
 * disbursement history on mount; neither is available in CI, so both are
 * faked. Nothing here touches the CSV parsing under test — that is entirely
 * client-side.
 */
async function openSdpTab(page: Page): Promise<void> {
  await page.context().addCookies([
    {
      name: "token",
      value: "e2e-mock-api-token",
      domain: "localhost",
      path: "/",
    },
    {
      name: "zaps-auth",
      value: "1",
      domain: "localhost",
      path: "/",
    },
  ]);
  await page.addInitScript(() => {
    const expiry = Date.now() + 60 * 60 * 1000;
    window.localStorage.setItem("privy:token", "e2e-mock-access-token");
    window.localStorage.setItem(
      "privy:session",
      JSON.stringify({ expiry, userId: "did:privy:e2e-user" }),
    );
    window.localStorage.setItem("token", "e2e-mock-api-token");
  });

  await page.route("**/api/**", (route) => {
    const url = route.request().url();
    if (url.includes("/sdp/") || url.includes("disbursement")) {
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ disbursements: [], data: [] }),
      });
    }
    return route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ data: [], batches: [], total: 0 }),
    });
  });

  await page.goto("/dashboard/payouts");
  await page.getByRole("button", { name: /sdp csv upload/i }).click();
  await expect(
    page.getByRole("heading", { name: /upload sdp disbursement csv/i }),
  ).toBeVisible({ timeout: 15_000 });
}

/** Feed a fixture into the dropzone's hidden file input. */
async function uploadFixture(page: Page, filename: string): Promise<void> {
  await page
    .locator('input[type="file"]')
    .setInputFiles(path.join(FIXTURES, filename));
}

// ── Upload and preview ───────────────────────────────────────────────────────

test.describe("SDP CSV upload", () => {
  test.beforeEach(async ({ page }) => {
    await openSdpTab(page);
  });

  test("shows the selected file once it is chosen", async ({ page }) => {
    await uploadFixture(page, "disbursement-valid.csv");

    await expect(page.getByText("disbursement-valid.csv")).toBeVisible({
      timeout: 10_000,
    });
  });

  test("renders the preview grid with the right recipient row count", async ({
    page,
  }) => {
    await uploadFixture(page, "disbursement-valid.csv");

    // The success banner names the number of rows that will actually be sent.
    await expect(
      page.getByText(
        new RegExp(`${VALID_ROW_COUNT} recipient row\\(s\\) ready for upload`, "i"),
      ),
    ).toBeVisible({ timeout: 15_000 });

    // And the preview table lists them.
    await expect(
      page.getByText(new RegExp(`valid rows \\(${VALID_ROW_COUNT}\\)`, "i")),
    ).toBeVisible();
  });

  test("lists every recipient from the file in the preview", async ({
    page,
  }) => {
    await uploadFixture(page, "disbursement-valid.csv");
    await expect(
      page.getByText(new RegExp(`valid rows \\(${VALID_ROW_COUNT}\\)`, "i")),
    ).toBeVisible({ timeout: 15_000 });

    // Row count is read off the table body rather than the banner, so a banner
    // that disagrees with the grid cannot pass.
    const validTable = page
      .locator("div")
      .filter({ hasText: new RegExp(`^Valid rows \\(${VALID_ROW_COUNT}\\)`) })
      .locator("table")
      .first();

    await expect(validTable.locator("tbody tr")).toHaveCount(VALID_ROW_COUNT);
  });

  test("does not show a preview before any file is chosen", async ({
    page,
  }) => {
    await expect(page.getByText(/recipient row\(s\) ready for upload/i)).toHaveCount(
      0,
    );
    await expect(page.getByText(/valid rows \(/i)).toHaveCount(0);
  });
});

// ── Validation ───────────────────────────────────────────────────────────────

test.describe("SDP CSV validation", () => {
  test.beforeEach(async ({ page }) => {
    await openSdpTab(page);
  });

  test("separates valid rows from rows that need fixing", async ({ page }) => {
    await uploadFixture(page, "disbursement-mixed.csv");

    await expect(
      page.getByText(
        new RegExp(
          `${MIXED_INVALID} row\\(s\\) have validation issues`,
          "i",
        ),
      ),
    ).toBeVisible({ timeout: 15_000 });

    await expect(
      page.getByText(new RegExp(`validation issues \\(${MIXED_INVALID} rows\\)`, "i")),
    ).toBeVisible();
    await expect(
      page.getByText(new RegExp(`valid rows \\(${MIXED_VALID}\\)`, "i")),
    ).toBeVisible();
  });

  test("explains why a row failed rather than just rejecting it", async ({
    page,
  }) => {
    await uploadFixture(page, "disbursement-mixed.csv");
    await expect(
      page.getByText(new RegExp(`validation issues \\(${MIXED_INVALID} rows\\)`, "i")),
    ).toBeVisible({ timeout: 15_000 });

    await expect(
      page.getByText(/stellar_address is not a valid G-address/i),
    ).toBeVisible();
    await expect(
      page.getByText(/amount must be a positive number/i),
    ).toBeVisible();
  });

  test("rejects a file whose columns SDP cannot use", async ({ page }) => {
    await uploadFixture(page, "disbursement-bad-headers.csv");

    await expect(
      page.getByText(/csv header validation failed/i),
    ).toBeVisible({ timeout: 15_000 });
    await expect(
      page.getByText(/missing required column group/i),
    ).toBeVisible();

    // A file the platform cannot read must not present a ready-to-send count.
    await expect(
      page.getByText(/recipient row\(s\) ready for upload/i),
    ).toHaveCount(0);
  });

  test("replaces the previous preview when a second file is chosen", async ({
    page,
  }) => {
    await uploadFixture(page, "disbursement-valid.csv");
    await expect(
      page.getByText(new RegExp(`valid rows \\(${VALID_ROW_COUNT}\\)`, "i")),
    ).toBeVisible({ timeout: 15_000 });

    await uploadFixture(page, "disbursement-mixed.csv");

    await expect(
      page.getByText(new RegExp(`valid rows \\(${MIXED_VALID}\\)`, "i")),
    ).toBeVisible({ timeout: 15_000 });
    // The first file's count must be gone, not stacked underneath.
    await expect(
      page.getByText(new RegExp(`valid rows \\(${VALID_ROW_COUNT}\\)`, "i")),
    ).toHaveCount(0);
  });
});
