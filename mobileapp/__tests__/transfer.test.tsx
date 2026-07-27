jest.mock("@react-native-async-storage/async-storage", () =>
  require("@react-native-async-storage/async-storage/jest/async-storage-mock")
);

import React from "react";
import { render, waitFor, act, fireEvent } from "@testing-library/react-native";
import TransferScreenWithBoundary from "../app/transfer";

// ── Mocks ──────────────────────────────────────────────────────────────────────

const mockFetch = jest.fn();
global.fetch = mockFetch;

jest.mock("expo-router", () => ({
  useRouter: jest.fn(() => ({
    back: jest.fn(),
    replace: jest.fn(),
  })),
  Stack: {
    Screen: () => null,
  },
}));

jest.mock("../src/services/stellarWallet", () => ({
  getLocalKeypair: jest.fn(() => null),
  checkFreighter: jest.fn(),
  connectFreighter: jest.fn(),
  connectAlbedo: jest.fn(),
  connectLocalWallet: jest.fn(),
  generateLocalKeypair: jest.fn(),
  saveLocalKeypair: jest.fn(),
  submitPayment: jest.fn(),
}));

jest.mock("../src/services/api", () => ({
  getRecentRecipients: jest.fn(() => Promise.resolve([])),
  saveRecentRecipient: jest.fn(),
}));

// Mock new upstream imports used by transfer.tsx (virtual because not in direct deps)
jest.mock("expo-document-picker", () => ({
  getDocumentAsync: jest.fn(),
}), { virtual: true });

jest.mock("expo-file-system", () => ({
  readAsStringAsync: jest.fn(),
  EncodingType: { UTF8: "utf8" },
  documentDirectory: "/mock",
}), { virtual: true });

jest.mock("../src/components/BatchPayoutItemRow", () => "BatchPayoutItemRow");
jest.mock("../src/components/BatchPayoutSummary", () => "BatchPayoutSummary");

// Mock SVG imports
jest.mock("../assets/icon-4.svg", () => "ZapsIcon");
jest.mock("../assets/wallet.svg", () => "WalletIcon");
jest.mock("../assets/XML-logo.svg", () => "XLMLogo");
jest.mock("../assets/USDT-logo.svg", () => "USDTLogo");
jest.mock("../assets/USDC-logo.svg", () => "USDCLogo");

// ── Helpers ─────────────────────────────────────────────────────────────────────

/** Advance past step 0 (select transfer type) to step 1 (recipient input). */
async function advanceToStep1(renderResult: ReturnType<typeof render>) {
  const { getByText } = renderResult;
  // Step 0 shows two AccountTypeCards; the "Zaps User" one is already selected
  // by default (transferType starts as "ZAPS"). Press "Continue".
  const continueBtn = getByText("Continue");
  await act(async () => {
    fireEvent.press(continueBtn);
  });
}

/**
 * Type a recipient username, advance past the debounce timer,
 * and tap the "1" keypad button to set a non-zero amount.
 */
async function typeRecipient(renderResult: ReturnType<typeof render>, username: string) {
  const { getByPlaceholderText } = renderResult;
  const input = getByPlaceholderText(/Recipient ZAPS ID/i);
  await act(async () => {
    fireEvent.changeText(input, username);
    // Wait for debounce (350ms) + any pending promises
    await new Promise((r) => setTimeout(r, 500));
  });

  // Tap a keypad digit so amount is non-empty (enables the continue button
  // when there is no warning)
  await act(async () => {
    fireEvent.press(renderResult.getByText("1"));
  });
}

// ── Tests ───────────────────────────────────────────────────────────────────────

describe("TransferScreen – Registry Warning Banners", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    // Default: search returns empty, resolve returns 404 (not registered)
    mockFetch.mockReset();
  });

  // ── 1. "Not registered" warning ────────────────────────────────────────────

  it("shows 'not registered' warning when resolve returns 404", async () => {
    // Mock the API calls that fire during step 1 recipient typing
    // Search call (GET /api/users/search) → empty results
    // Resolve call (GET /api/users/resolve/:username) → 404
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => [],
      })
      .mockResolvedValueOnce({
        ok: false,
        status: 404,
        json: async () => ({ error: "username not found" }),
      });

    const renderResult = render(<TransferScreenWithBoundary />);
    await advanceToStep1(renderResult);
    await typeRecipient(renderResult, "unknownuser");

    await waitFor(() => {
      expect(renderResult.getByTestId("warning-not-registered")).toBeTruthy();
    });

    // Verify the warning text includes the username
    expect(
      renderResult.getByText(/unknownuser is not registered on Zaps/)
    ).toBeTruthy();
  });

  it("disables continue button when username is not registered", async () => {
    // Provide enough state so the button WOULD be enabled otherwise
    // (recipient + amount filled), then show the warning
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => [],
      })
      .mockResolvedValueOnce({
        ok: false,
        status: 404,
        json: async () => ({ error: "username not found" }),
      });

    const renderResult = render(<TransferScreenWithBoundary />);
    await advanceToStep1(renderResult);
    await typeRecipient(renderResult, "ghostuser");

    // Wait for the warning to appear
    await waitFor(() => {
      expect(renderResult.getByTestId("warning-not-registered")).toBeTruthy();
    });

    // The continue button should be disabled
    // Use getByLabelText (which gets the TouchableOpacity with accessibilityLabel="Review")
    // instead of getByText (which returns the inner <Text> without accessibilityState).
    const continueBtn = renderResult.getByLabelText("Review");
    expect(continueBtn.props.accessibilityState?.disabled).toBe(true);
  });

  // ── 2. "Blacklisted" warning ───────────────────────────────────────────────

  it("shows 'blacklisted' warning when resolve returns is_blacklisted=true", async () => {
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => [],
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({
          username: "flaggeduser",
          address: "GABCDEF1234567890XYZ",
          is_blacklisted: true,
        }),
      });

    const renderResult = render(<TransferScreenWithBoundary />);
    await advanceToStep1(renderResult);
    await typeRecipient(renderResult, "flaggeduser");

    await waitFor(() => {
      expect(renderResult.getByTestId("warning-blacklisted")).toBeTruthy();
    });

    expect(
      renderResult.getByText(/flaggeduser has been flagged/)
    ).toBeTruthy();
  });

  it("disables continue button when username is blacklisted", async () => {
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => [],
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({
          username: "banneduser",
          address: "GABCDEF1234567890XYZ",
          is_blacklisted: true,
        }),
      });

    const renderResult = render(<TransferScreenWithBoundary />);
    await advanceToStep1(renderResult);
    await typeRecipient(renderResult, "banneduser");

    await waitFor(() => {
      expect(renderResult.getByTestId("warning-blacklisted")).toBeTruthy();
    });

    const continueBtn = renderResult.getByLabelText("Review");
    expect(continueBtn.props.accessibilityState?.disabled).toBe(true);
  });

  // ── 3. "Registered" confirmation (no warning) ──────────────────────────────

  it("shows 'registered' confirmation when resolve returns 200 OK", async () => {
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => [{ username: "existinguser", address: "GXXXX" }],
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({
          username: "existinguser",
          address: "GABCDEF1234567890XYZ",
        }),
      });

    const renderResult = render(<TransferScreenWithBoundary />);
    await advanceToStep1(renderResult);
    await typeRecipient(renderResult, "existinguser");

    await waitFor(() => {
      expect(renderResult.getByTestId("warning-registered")).toBeTruthy();
    });

    expect(
      renderResult.getByText(/existinguser is a registered Zaps user/)
    ).toBeTruthy();
  });

  it("enables continue button when username is registered", async () => {
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => [{ username: "validuser", address: "GXXXX" }],
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({
          username: "validuser",
          address: "GABCDEF1234567890XYZ",
        }),
      });

    const renderResult = render(<TransferScreenWithBoundary />);
    await advanceToStep1(renderResult);
    await typeRecipient(renderResult, "validuser");

    await waitFor(() => {
      expect(renderResult.getByTestId("warning-registered")).toBeTruthy();
    });

    // The continue button should NOT be disabled for registered users
    const continueBtn = renderResult.getByLabelText("Review");
    const state = continueBtn.props.accessibilityState;
    expect(state?.disabled).not.toBe(true);
  });

  // ── 4. Edge cases ──────────────────────────────────────────────────────────

  it("shows checking indicator while resolve request is in flight", async () => {
    // Return a promise that doesn't resolve immediately so we can observe "checking"
    let resolvePromise!: (value: any) => void;
    const pendingPromise = new Promise<any>((resolve) => {
      resolvePromise = resolve;
    });

    // Mock for the search call (resolves immediately)
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => [],
      })
      // Resolve call stays pending so we can see "checking" state
      .mockReturnValueOnce(pendingPromise);

    const renderResult = render(<TransferScreenWithBoundary />);
    await advanceToStep1(renderResult);

    // Type a username — this triggers the debounced resolve call
    const input = renderResult.getByPlaceholderText(/Recipient ZAPS ID/i);
    fireEvent.changeText(input, "newuser");

    // Wait for the debounce timer to fire (350ms) + a small buffer
    await act(async () => {
      await new Promise((r) => setTimeout(r, 500));
    });

    // The checking banner should be visible while the request is pending
    expect(renderResult.getByTestId("warning-checking")).toBeTruthy();

    // Resolve the pending request
    await act(async () => {
      resolvePromise({
        ok: false,
        status: 404,
        json: async () => ({ error: "username not found" }),
      });
      await new Promise((r) => setTimeout(r, 50));
    });

    // Now the "not registered" warning should appear
    await waitFor(() => {
      expect(renderResult.getByTestId("warning-not-registered")).toBeTruthy();
    });
  });

  it("does not show any warning for external wallet mode", async () => {
    // Type a recipient — no warning should appear for external wallets
    const renderResult = render(<TransferScreenWithBoundary />);
    await advanceToStep1(renderResult);
    await typeRecipient(renderResult, "someusername");

    // The warning banner condition requires transferType === "ZAPS",
    // so no testIDs should be present.
    expect(renderResult.queryByTestId("warning-not-registered")).toBeNull();
    expect(renderResult.queryByTestId("warning-blacklisted")).toBeNull();
    expect(renderResult.queryByTestId("warning-registered")).toBeNull();
  });

  it("clears warning when user edits the recipient field", async () => {
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => [],
      })
      .mockResolvedValueOnce({
        ok: false,
        status: 404,
        json: async () => ({ error: "username not found" }),
      });

    const renderResult = render(<TransferScreenWithBoundary />);
    await advanceToStep1(renderResult);
    await typeRecipient(renderResult, "ghostuser");

    await waitFor(() => {
      expect(renderResult.getByTestId("warning-not-registered")).toBeTruthy();
    });

    // Now edit the field — warning should disappear
    await act(async () => {
      const input = renderResult.getByPlaceholderText(/Recipient ZAPS ID/i);
      fireEvent.changeText(input, "ghostuser_edited");
      await new Promise((r) => setTimeout(r, 50));
    });

    // The "not registered" warning should be gone (replaced by idle/checking)
    expect(renderResult.queryByTestId("warning-not-registered")).toBeNull();
  });
});
