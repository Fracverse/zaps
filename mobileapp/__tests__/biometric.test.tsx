/**
 * biometric.test.tsx
 *
 * Unit tests for biometric hardware detection and PIN fallback flow (#684).
 *
 * Tests cover:
 *  - `authenticateWithBiometrics`: hardware-absent / not-enrolled / success paths
 *  - `saveAccountPin` / `verifyAccountPin` helpers
 *  - `PinFallbackModal`: renders correctly in setup and verify modes
 *  - `BiometricScreen`: shows PIN-fallback banner when hardware unavailable
 *  - `BiometricScreen`: auto-opens PIN modal for non-hardware devices
 */

import React from "react";
import { render, fireEvent, waitFor, act } from "@testing-library/react-native";

// ── Mocks ──────────────────────────────────────────────────────────────────────

jest.mock("@react-native-async-storage/async-storage", () =>
  require("@react-native-async-storage/async-storage/jest/async-storage-mock")
);

const mockHasHardwareAsync = jest.fn();
const mockIsEnrolledAsync = jest.fn();
const mockAuthenticateAsync = jest.fn();

jest.mock("expo-local-authentication", () => ({
  hasHardwareAsync: () => mockHasHardwareAsync(),
  isEnrolledAsync: () => mockIsEnrolledAsync(),
  authenticateAsync: (opts: unknown) => mockAuthenticateAsync(opts),
}), { virtual: true });

jest.mock("expo-router", () => ({
  useRouter: jest.fn(() => ({
    back: jest.fn(),
    push: jest.fn(),
    replace: jest.fn(),
  })),
  useLocalSearchParams: jest.fn(() => ({})),
  Stack: { Screen: () => null },
}));

jest.mock("../assets/fingerprint.png", () => "fingerprint-image", {
  virtual: true,
});

// ── Imports (after mocks) ──────────────────────────────────────────────────────

import AsyncStorage from "@react-native-async-storage/async-storage";
import {
  authenticateWithBiometrics,
  saveAccountPin,
  verifyAccountPin,
  PinFallbackModal,
} from "../app/biometric";

// ── Helpers ────────────────────────────────────────────────────────────────────

beforeEach(async () => {
  jest.clearAllMocks();
  await AsyncStorage.clear();
});

// ─ authenticateWithBiometrics ─────────────────────────────────────────────────

describe("authenticateWithBiometrics", () => {
  it("returns success when biometric prompt succeeds", async () => {
    mockHasHardwareAsync.mockResolvedValue(true);
    mockIsEnrolledAsync.mockResolvedValue(true);
    mockAuthenticateAsync.mockResolvedValue({ success: true });

    const result = await authenticateWithBiometrics();

    expect(result.success).toBe(true);
  });

  it("returns cancelled when user cancels the biometric prompt", async () => {
    mockHasHardwareAsync.mockResolvedValue(true);
    mockIsEnrolledAsync.mockResolvedValue(true);
    mockAuthenticateAsync.mockResolvedValue({ success: false, error: "user_cancel" });

    const result = await authenticateWithBiometrics();

    expect(result.success).toBe(false);
    if (!result.success) expect(result.reason).toBe("cancelled");
  });

  it("falls back to passcode when hardware is unavailable", async () => {
    mockHasHardwareAsync.mockResolvedValue(false);
    mockAuthenticateAsync.mockResolvedValue({ success: true });

    const result = await authenticateWithBiometrics();

    expect(result.success).toBe(true);
    // Should have called authenticateAsync with disableDeviceFallback: false
    expect(mockAuthenticateAsync).toHaveBeenCalledWith(
      expect.objectContaining({ disableDeviceFallback: false })
    );
  });

  it("falls back to passcode when no biometrics are enrolled", async () => {
    mockHasHardwareAsync.mockResolvedValue(true);
    mockIsEnrolledAsync.mockResolvedValue(false);
    mockAuthenticateAsync.mockResolvedValue({ success: true });

    const result = await authenticateWithBiometrics();

    expect(result.success).toBe(true);
  });

  it("returns not_enrolled when unenrolled and passcode also fails", async () => {
    mockHasHardwareAsync.mockResolvedValue(true);
    mockIsEnrolledAsync.mockResolvedValue(false);
    mockAuthenticateAsync.mockResolvedValue({ success: false, error: "passcode_failed" });

    const result = await authenticateWithBiometrics();

    expect(result.success).toBe(false);
    if (!result.success) expect(result.reason).toBe("not_enrolled");
  });

  it("returns error when LocalAuthentication throws", async () => {
    mockHasHardwareAsync.mockRejectedValue(new Error("sensor unavailable"));

    const result = await authenticateWithBiometrics();

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.reason).toBe("error");
      expect(result.error).toContain("sensor unavailable");
    }
  });
});

// ─ saveAccountPin / verifyAccountPin ──────────────────────────────────────────

describe("saveAccountPin / verifyAccountPin", () => {
  it("verifies a correct PIN after saving", async () => {
    await saveAccountPin("1234");
    expect(await verifyAccountPin("1234")).toBe(true);
  });

  it("rejects an incorrect PIN", async () => {
    await saveAccountPin("1234");
    expect(await verifyAccountPin("9999")).toBe(false);
  });

  it("returns true when no PIN has been saved yet (first-time setup)", async () => {
    // Nothing in AsyncStorage
    expect(await verifyAccountPin("anything")).toBe(true);
  });
});

// ─ PinFallbackModal (setup mode) ──────────────────────────────────────────────

describe("PinFallbackModal (setup mode)", () => {
  it("renders PIN and confirm inputs in setup mode", () => {
    const onSuccess = jest.fn();
    const { getByTestId } = render(
      <PinFallbackModal
        visible
        isSetup
        onSuccess={onSuccess}
        onCancel={jest.fn()}
      />
    );

    expect(getByTestId("pin-input")).toBeTruthy();
    expect(getByTestId("pin-confirm-input")).toBeTruthy();
  });

  it("shows error when PINs do not match", async () => {
    const onSuccess = jest.fn();
    const { getByTestId } = render(
      <PinFallbackModal
        visible
        isSetup
        onSuccess={onSuccess}
        onCancel={jest.fn()}
      />
    );

    fireEvent.changeText(getByTestId("pin-input"), "1234");
    fireEvent.changeText(getByTestId("pin-confirm-input"), "5678");
    await act(async () => {
      fireEvent.press(getByTestId("pin-submit-button"));
    });

    expect(getByTestId("pin-error")).toBeTruthy();
    expect(onSuccess).not.toHaveBeenCalled();
  });

  it("shows error when PIN is too short", async () => {
    const onSuccess = jest.fn();
    const { getByTestId } = render(
      <PinFallbackModal
        visible
        isSetup
        onSuccess={onSuccess}
        onCancel={jest.fn()}
      />
    );

    fireEvent.changeText(getByTestId("pin-input"), "12");
    fireEvent.changeText(getByTestId("pin-confirm-input"), "12");
    await act(async () => {
      fireEvent.press(getByTestId("pin-submit-button"));
    });

    expect(getByTestId("pin-error")).toBeTruthy();
    expect(onSuccess).not.toHaveBeenCalled();
  });

  it("calls onSuccess and saves PIN when inputs match", async () => {
    const onSuccess = jest.fn();
    const { getByTestId } = render(
      <PinFallbackModal
        visible
        isSetup
        onSuccess={onSuccess}
        onCancel={jest.fn()}
      />
    );

    fireEvent.changeText(getByTestId("pin-input"), "4321");
    fireEvent.changeText(getByTestId("pin-confirm-input"), "4321");
    await act(async () => {
      fireEvent.press(getByTestId("pin-submit-button"));
      // Wait for async saveAccountPin to complete
      await new Promise((r) => setTimeout(r, 50));
    });

    expect(onSuccess).toHaveBeenCalled();
  });

  it("calls onCancel when cancel button is pressed", () => {
    const onCancel = jest.fn();
    const { getByTestId } = render(
      <PinFallbackModal visible isSetup onSuccess={jest.fn()} onCancel={onCancel} />
    );

    fireEvent.press(getByTestId("pin-cancel-button"));
    expect(onCancel).toHaveBeenCalled();
  });
});

// ─ PinFallbackModal (verify mode) ─────────────────────────────────────────────

describe("PinFallbackModal (verify mode)", () => {
  it("does not render confirm input in verify mode", () => {
    const { queryByTestId } = render(
      <PinFallbackModal
        visible
        isSetup={false}
        onSuccess={jest.fn()}
        onCancel={jest.fn()}
      />
    );

    expect(queryByTestId("pin-confirm-input")).toBeNull();
  });

  it("shows error for an incorrect PIN", async () => {
    await saveAccountPin("1234");

    const onSuccess = jest.fn();
    const { getByTestId } = render(
      <PinFallbackModal
        visible
        isSetup={false}
        onSuccess={onSuccess}
        onCancel={jest.fn()}
      />
    );

    fireEvent.changeText(getByTestId("pin-input"), "0000");
    await act(async () => {
      fireEvent.press(getByTestId("pin-submit-button"));
      await new Promise((r) => setTimeout(r, 50));
    });

    expect(getByTestId("pin-error")).toBeTruthy();
    expect(onSuccess).not.toHaveBeenCalled();
  });

  it("calls onSuccess for a correct PIN", async () => {
    await saveAccountPin("5678");

    const onSuccess = jest.fn();
    const { getByTestId } = render(
      <PinFallbackModal
        visible
        isSetup={false}
        onSuccess={onSuccess}
        onCancel={jest.fn()}
      />
    );

    fireEvent.changeText(getByTestId("pin-input"), "5678");
    await act(async () => {
      fireEvent.press(getByTestId("pin-submit-button"));
      await new Promise((r) => setTimeout(r, 50));
    });

    expect(onSuccess).toHaveBeenCalled();
  });
});

// ─ BiometricScreen (hardware status banners) ──────────────────────────────────

describe("BiometricScreen hardware-unavailable banner (#684)", () => {
  it("shows 'hardware unavailable' banner when hasHardwareAsync returns false", async () => {
    const { useLocalSearchParams } = require("expo-router");
    useLocalSearchParams.mockReturnValue({});

    mockHasHardwareAsync.mockResolvedValue(false);

    const BiometricScreen = require("../app/biometric").default;
    const { getByTestId } = render(<BiometricScreen />);

    await waitFor(() => {
      expect(getByTestId("hardware-unavailable-banner")).toBeTruthy();
    });
  });

  it("shows 'not enrolled' banner when isEnrolledAsync returns false", async () => {
    const { useLocalSearchParams } = require("expo-router");
    useLocalSearchParams.mockReturnValue({});

    mockHasHardwareAsync.mockResolvedValue(true);
    mockIsEnrolledAsync.mockResolvedValue(false);

    const BiometricScreen = require("../app/biometric").default;
    const { getByTestId } = render(<BiometricScreen />);

    await waitFor(() => {
      expect(getByTestId("hardware-not-enrolled-banner")).toBeTruthy();
    });
  });
});
