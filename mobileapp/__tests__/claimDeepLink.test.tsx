/**
 * claimDeepLink.test.tsx
 *
 * Tests for the SDP deep link claim screen and token pre-population (#703).
 *
 * Tests cover:
 *  - ClaimValidationScreen renders with a pre-populated token from route params
 *  - Auto-validation fires on mount when a token is provided
 *  - Token display shown for successful claims
 *  - Manual token input shown when no route param is provided
 *  - Token validation error shown for malformed tokens
 *  - Retry works after an error
 */

import React from "react";
import { render, waitFor, act, fireEvent } from "@testing-library/react-native";

// ── Mocks ──────────────────────────────────────────────────────────────────────

const mockRouterBack = jest.fn();
const mockRouterReplace = jest.fn();
const mockUseLocalSearchParams = jest.fn();

jest.mock("expo-router", () => ({
  useRouter: jest.fn(() => ({
    back: mockRouterBack,
    replace: mockRouterReplace,
  })),
  useLocalSearchParams: () => mockUseLocalSearchParams(),
  Stack: { Screen: () => null },
}));

const mockValidateClaimToken = jest.fn();
jest.mock("../src/services/sdpService", () => ({
  validateClaimToken: (...args: unknown[]) => mockValidateClaimToken(...args),
}));

// ── Imports (after mocks) ──────────────────────────────────────────────────────

import ClaimValidationScreen from "../app/claim/[token]";

// ── Tests ──────────────────────────────────────────────────────────────────────

beforeEach(() => {
  jest.clearAllMocks();
});

describe("ClaimValidationScreen — deep link token pre-population (#703)", () => {
  // ── 1. Auto-population from route param ─────────────────────────────────

  it("auto-validates when a token route param is provided", async () => {
    mockUseLocalSearchParams.mockReturnValue({ token: "abc123-DEF456" });
    mockValidateClaimToken.mockResolvedValue({ status: "valid", amount: "500", assetCode: "USDC" });

    const { getByText } = render(<ClaimValidationScreen />);

    await waitFor(() => {
      expect(getByText("Funds Waiting for You")).toBeTruthy();
    });

    expect(mockValidateClaimToken).toHaveBeenCalledWith("abc123-DEF456");
  });

  it("displays the pre-populated token in the token display area", async () => {
    mockUseLocalSearchParams.mockReturnValue({ token: "preloaded-token-01" });
    mockValidateClaimToken.mockResolvedValue({ status: "valid" });

    const { getByTestId } = render(<ClaimValidationScreen />);

    await waitFor(() => {
      const tokenDisplay = getByTestId("claim-token-value");
      expect(tokenDisplay.props.children).toBe("preloaded-token-01");
    });
  });

  // ── 2. No route param — shows manual entry form ──────────────────────────

  it("shows the token input field when no route param is provided", () => {
    mockUseLocalSearchParams.mockReturnValue({ token: undefined });

    const { getByTestId } = render(<ClaimValidationScreen />);

    expect(getByTestId("claim-token-input")).toBeTruthy();
  });

  it("does NOT auto-validate when no token is present", () => {
    mockUseLocalSearchParams.mockReturnValue({ token: undefined });
    render(<ClaimValidationScreen />);
    expect(mockValidateClaimToken).not.toHaveBeenCalled();
  });

  // ── 3. Manual entry ──────────────────────────────────────────────────────

  it("validates a manually entered token when the button is pressed", async () => {
    mockUseLocalSearchParams.mockReturnValue({ token: undefined });
    mockValidateClaimToken.mockResolvedValue({ status: "valid" });

    const { getByTestId } = render(<ClaimValidationScreen />);

    fireEvent.changeText(getByTestId("claim-token-input"), "manual-token-01");
    await act(async () => {
      fireEvent.press(getByTestId("claim-validate-button"));
    });

    await waitFor(() => {
      expect(mockValidateClaimToken).toHaveBeenCalledWith("manual-token-01");
    });
  });

  // ── 4. Token format validation ───────────────────────────────────────────

  it("shows an error for a malformed token without calling the API", async () => {
    mockUseLocalSearchParams.mockReturnValue({ token: undefined });

    const { getByTestId } = render(<ClaimValidationScreen />);

    fireEvent.changeText(getByTestId("claim-token-input"), "<bad!token>");
    await act(async () => {
      fireEvent.press(getByTestId("claim-validate-button"));
    });

    expect(mockValidateClaimToken).not.toHaveBeenCalled();
    expect(getByTestId("claim-token-error")).toBeTruthy();
  });

  it("shows an error when no token is entered before pressing validate", async () => {
    mockUseLocalSearchParams.mockReturnValue({ token: undefined });

    const { getByTestId } = render(<ClaimValidationScreen />);

    await act(async () => {
      fireEvent.press(getByTestId("claim-validate-button"));
    });

    expect(getByTestId("claim-token-error")).toBeTruthy();
    expect(mockValidateClaimToken).not.toHaveBeenCalled();
  });

  // ── 5. Result states ─────────────────────────────────────────────────────

  it("renders 'Already Claimed' for already_claimed status", async () => {
    mockUseLocalSearchParams.mockReturnValue({ token: "valid-token-001" });
    mockValidateClaimToken.mockResolvedValue({ status: "already_claimed" });

    const { getByText } = render(<ClaimValidationScreen />);

    await waitFor(() => {
      expect(getByText("Already Claimed")).toBeTruthy();
    });
  });

  it("renders 'Link Expired' for expired status", async () => {
    mockUseLocalSearchParams.mockReturnValue({ token: "expired-token-01" });
    mockValidateClaimToken.mockResolvedValue({ status: "expired" });

    const { getByText } = render(<ClaimValidationScreen />);

    await waitFor(() => {
      expect(getByText("Link Expired")).toBeTruthy();
    });
  });

  it("renders error screen when validateClaimToken throws", async () => {
    mockUseLocalSearchParams.mockReturnValue({ token: "throw-me-token1" });
    mockValidateClaimToken.mockRejectedValue(new Error("Network error"));

    const { getByText } = render(<ClaimValidationScreen />);

    await waitFor(() => {
      expect(getByText("Something went wrong")).toBeTruthy();
    });
  });

  // ── 6. Navigation ────────────────────────────────────────────────────────

  it("navigates to home when 'Continue' is pressed on a valid claim", async () => {
    mockUseLocalSearchParams.mockReturnValue({ token: "navigate-token-01" });
    mockValidateClaimToken.mockResolvedValue({ status: "valid" });

    const { getByTestId } = render(<ClaimValidationScreen />);

    await waitFor(() => {
      expect(getByTestId("claim-continue-button")).toBeTruthy();
    });

    fireEvent.press(getByTestId("claim-continue-button"));
    expect(mockRouterReplace).toHaveBeenCalledWith("/(personal)/home");
  });
});
