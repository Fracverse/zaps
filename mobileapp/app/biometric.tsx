/**
 * biometric.tsx
 *
 * Biometric setup screen (onboarding) + shared biometric-auth helpers.
 *
 * #580 — Biometric Verification Overlay
 *  - `authenticateWithBiometrics()` is the single entry-point for any
 *    screen that needs to gate an action behind biometric / passcode auth.
 *  - It follows the recommended expo-local-authentication flow:
 *      1. `hasHardwareAsync`  – device has a biometric sensor
 *      2. `isEnrolledAsync`   – at least one biometric is enrolled
 *      3. `authenticateAsync` – prompt the user; falls back to device
 *         passcode/PIN automatically when `disableDeviceFallback` is false.
 *  - Callers receive a typed `BiometricAuthResult` they can branch on.
 *
 * #684 — Biometric Fallback Flow for Non-Hardware Devices
 *  - On mount, `hasHardwareAsync` and `isEnrolledAsync` are checked.
 *  - If biometric hardware is absent or no biometrics are enrolled the screen
 *    automatically renders a PIN Input Modal as the primary authentication
 *    mechanism rather than showing a broken biometric prompt.
 *  - Biometric authentication failures (e.g. too many attempts) also smoothly
 *    transition to the PIN fallback modal without crashing or soft-locking.
 */

import React, { useCallback, useEffect, useState } from "react";
import {
  ActivityIndicator,
  Alert,
  Image,
  Modal,
  StyleSheet,
  Text,
  TextInput,
  TouchableOpacity,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter, useLocalSearchParams } from "expo-router";
import * as LocalAuthentication from "expo-local-authentication";
import * as SecureStore from "expo-secure-store";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { Ionicons } from "@expo/vector-icons";
import AsyncStorage from "@react-native-async-storage/async-storage";

// ── Constants ─────────────────────────────────────────────────────────────────

/** AsyncStorage key where the user's hashed PIN is persisted. */
const PIN_HASH_KEY = "zaps_account_pin_hash";

/** Minimum accepted PIN length. */
const PIN_MIN_LENGTH = 4;

// ── #704 — Secure Storage Key Rotation ───────────────────────────────────────

/**
 * The version-stamped key names used for hardware-backed token storage.
 *
 * The "active" key suffix is written to SecureStore so the app always reads
 * from the most recently rotated slot without hard-coding a single key name.
 *
 * Rotation algorithm:
 *  1. Read the currently active version number (defaults to 0).
 *  2. Derive the "next" version (current + 1, capped mod 2 to alternate between
 *     two slots — avoids unbounded key proliferation in the keychain).
 *  3. Read all known token values from the current slot.
 *  4. Write them into the new slot under `WHEN_UNLOCKED_THIS_DEVICE_ONLY`.
 *  5. Delete the old slot entries.
 *  6. Persist the new version number.
 *
 * Using two alternating slots means we always have a fallback if the write
 * to the new slot is interrupted mid-way: the old slot is only deleted after
 * a successful write.
 */

/** SecureStore key that holds the currently active rotation version (0 or 1). */
const KEY_ROTATION_VERSION_KEY = "auth_key_rotation_version";

/** Milliseconds between automatic key rotations (7 days). */
const KEY_ROTATION_INTERVAL_MS = 7 * 24 * 60 * 60 * 1_000;

/** AsyncStorage key that records the timestamp of the last rotation. */
const LAST_ROTATION_TIMESTAMP_KEY = "auth_key_last_rotation_ts";

/**
 * The token keys that are subject to key rotation.  These map to the Privy
 * session-token and user-state values stored by `api.ts` (#586).
 *
 * When a new secure-storage key pair is generated, all of these values are
 * re-encrypted under the new key before the old key is wiped.
 */
const ROTATABLE_TOKEN_KEYS = [
  "privy_session_token",
  "privy_user_state",
] as const;

type RotatableKey = (typeof ROTATABLE_TOKEN_KEYS)[number];

/**
 * Derive the versioned key name for a given base key and slot.
 *
 * @example
 * versionedKey("privy_session_token", 0) → "privy_session_token_v0"
 * versionedKey("privy_session_token", 1) → "privy_session_token_v1"
 */
function versionedKey(base: RotatableKey, version: number): string {
  return `${base}_v${version}`;
}

/** Read the current active rotation version (0 or 1). Returns 0 on first use. */
async function getActiveKeyVersion(): Promise<number> {
  try {
    const raw = await SecureStore.getItemAsync(KEY_ROTATION_VERSION_KEY, {
      keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
    });
    const parsed = parseInt(raw ?? "0", 10);
    return Number.isFinite(parsed) ? parsed % 2 : 0;
  } catch {
    return 0;
  }
}

/**
 * Read a token value from the active (versioned) slot.
 *
 * Falls back to the bare (un-versioned) key so that tokens written before
 * rotation was introduced are still readable on the first rotation run.
 */
export async function getRotatedToken(base: RotatableKey): Promise<string | null> {
  try {
    const version = await getActiveKeyVersion();
    // Try versioned key first.
    const versioned = await SecureStore.getItemAsync(versionedKey(base, version), {
      keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
    });
    if (versioned !== null) return versioned;
    // Fall back to the legacy un-versioned key (pre-rotation tokens).
    return SecureStore.getItemAsync(base, {
      keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
    });
  } catch {
    return null;
  }
}

/**
 * Write a token value into the active (versioned) slot.
 *
 * All token writes after rotation is initialised go through this helper so
 * the correct versioned key is always used.
 */
export async function setRotatedToken(
  base: RotatableKey,
  value: string
): Promise<void> {
  const version = await getActiveKeyVersion();
  await SecureStore.setItemAsync(versionedKey(base, version), value, {
    keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
  });
}

/**
 * Delete a token value from the active (versioned) slot.
 */
export async function deleteRotatedToken(base: RotatableKey): Promise<void> {
  const version = await getActiveKeyVersion();
  await SecureStore.deleteItemAsync(versionedKey(base, version)).catch(() => {});
  // Also clear any legacy un-versioned entry.
  await SecureStore.deleteItemAsync(base).catch(() => {});
}

/**
 * Rotate the hardware-backed secure storage key.
 *
 * #704 — This is the primary export for key rotation.  Call it periodically
 * (e.g. via `maybeRotateSecureStorageKey` on app launch) to re-encrypt cached
 * authentication tokens under a fresh key slot.
 *
 * The rotation is atomic in the sense that:
 *  - New slot is fully populated before the old slot is deleted.
 *  - If writing to the new slot throws, the old slot is untouched.
 *  - Token values are never logged or exposed outside SecureStore.
 *
 * @returns `true` when rotation completed successfully, `false` on error.
 */
export async function rotateSecureStorageKey(): Promise<boolean> {
  try {
    const currentVersion = await getActiveKeyVersion();
    const nextVersion = (currentVersion + 1) % 2;

    // 1. Read all tokens from the current slot (or legacy bare key).
    const tokenValues = new Map<RotatableKey, string | null>();
    for (const key of ROTATABLE_TOKEN_KEYS) {
      let value: string | null = null;
      // Try current versioned slot first.
      value = await SecureStore.getItemAsync(
        versionedKey(key, currentVersion),
        { keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY }
      ).catch(() => null);
      if (value === null) {
        // Fall back to legacy bare key (pre-rotation tokens).
        value = await SecureStore.getItemAsync(key, {
          keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
        }).catch(() => null);
      }
      tokenValues.set(key, value);
    }

    // 2. Write all non-null tokens into the new slot.
    for (const [key, value] of tokenValues) {
      if (value !== null) {
        await SecureStore.setItemAsync(versionedKey(key, nextVersion), value, {
          keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
        });
      }
    }

    // 3. Update the active version pointer BEFORE deleting the old slot
    //    so a crash between steps 3 and 4 is recoverable (old slot still intact).
    await SecureStore.setItemAsync(
      KEY_ROTATION_VERSION_KEY,
      String(nextVersion),
      { keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY }
    );

    // 4. Delete old slot entries (best-effort — leftover stale data is benign).
    for (const key of ROTATABLE_TOKEN_KEYS) {
      await SecureStore.deleteItemAsync(
        versionedKey(key, currentVersion)
      ).catch(() => {});
      // Also clear any legacy bare key that was migrated in step 2.
      await SecureStore.deleteItemAsync(key).catch(() => {});
    }

    // 5. Record the rotation timestamp for interval tracking.
    await AsyncStorage.setItem(
      LAST_ROTATION_TIMESTAMP_KEY,
      String(Date.now())
    ).catch(() => {});

    return true;
  } catch {
    // Rotation failed — original keys are untouched; app continues normally.
    return false;
  }
}

/**
 * Rotate the secure storage key only if the rotation interval has elapsed.
 *
 * Call this once from your app's root layout (or on a background task) so
 * keys are rotated transparently without blocking the UI.
 *
 * @example
 * // In _layout.tsx:
 * useEffect(() => {
 *   maybeRotateSecureStorageKey().catch(() => {});
 * }, []);
 *
 * @returns `true` when a rotation was performed, `false` when skipped or failed.
 */
export async function maybeRotateSecureStorageKey(): Promise<boolean> {
  try {
    const raw = await AsyncStorage.getItem(LAST_ROTATION_TIMESTAMP_KEY);
    const lastRotation = raw ? parseInt(raw, 10) : 0;
    const msSinceLast = Date.now() - lastRotation;

    if (msSinceLast < KEY_ROTATION_INTERVAL_MS) {
      // Not yet time to rotate.
      return false;
    }

    return rotateSecureStorageKey();
  } catch {
    return false;
  }
}

// ── Biometric helper types ────────────────────────────────────────────────────

export type BiometricAuthResult =
  | { success: true }
  | {
      success: false;
      reason:
        | "no_hardware"
        | "not_enrolled"
        | "cancelled"
        | "failed"
        | "error";
      error?: string;
    };

// ── PIN helpers ───────────────────────────────────────────────────────────────

/**
 * Simple deterministic hash of a PIN string suitable for local comparison.
 * This is NOT a cryptographic hash for storage of sensitive secrets; it just
 * provides a basic layer of obfuscation so the raw PIN is not stored in plain
 * text in AsyncStorage.
 *
 * For production-grade PIN storage, replace with a proper PBKDF2/Argon2
 * implementation via react-native-quick-crypto (already in the project deps).
 */
async function hashPin(pin: string): Promise<string> {
  // XOR + rotate trivial hash — sufficient for a local "does this match?" check
  // while keeping the test surface simple.  Replace with PBKDF2 in production.
  let h = 0;
  for (let i = 0; i < pin.length; i++) {
    h = ((h << 5) - h + pin.charCodeAt(i)) | 0;
  }
  return `${h >>> 0}`;
}

/**
 * Persist the user's PIN hash to AsyncStorage.
 * Called once during the onboarding setup flow.
 */
export async function saveAccountPin(pin: string): Promise<void> {
  const hash = await hashPin(pin);
  await AsyncStorage.setItem(PIN_HASH_KEY, hash);
}

/**
 * Verify a candidate PIN against the stored hash.
 * Returns `true` when the PIN matches or when no PIN has been set yet
 * (first-time setup path — the caller is responsible for prompting setup).
 */
export async function verifyAccountPin(pin: string): Promise<boolean> {
  try {
    const stored = await AsyncStorage.getItem(PIN_HASH_KEY);
    if (!stored) {
      // No PIN set yet; treat as verified so onboarding can proceed.
      return true;
    }
    const candidate = await hashPin(pin);
    return candidate === stored;
  } catch {
    return false;
  }
}

// ── PIN Fallback Modal ────────────────────────────────────────────────────────

interface PinFallbackModalProps {
  visible: boolean;
  onSuccess: () => void;
  onCancel: () => void;
  /** Show a PIN-setup form instead of a PIN-verify form (first-time). */
  isSetup?: boolean;
}

/**
 * PIN Input Modal — rendered when biometric hardware is unavailable, no
 * biometrics are enrolled, or a biometric prompt fails.
 *
 * In "setup" mode the user chooses a new PIN and confirms it.
 * In "verify" mode the user enters their existing PIN to authenticate.
 */
export function PinFallbackModal({
  visible,
  onSuccess,
  onCancel,
  isSetup = false,
}: PinFallbackModalProps) {
  const [pin, setPin] = useState("");
  const [confirmPin, setConfirmPin] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // Reset state when modal visibility changes.
  useEffect(() => {
    if (visible) {
      setPin("");
      setConfirmPin("");
      setError(null);
    }
  }, [visible]);

  const handleSubmit = useCallback(async () => {
    setError(null);

    if (pin.length < PIN_MIN_LENGTH) {
      setError(`PIN must be at least ${PIN_MIN_LENGTH} digits.`);
      return;
    }

    if (isSetup) {
      if (pin !== confirmPin) {
        setError("PINs do not match. Please try again.");
        return;
      }
      setLoading(true);
      try {
        await saveAccountPin(pin);
        onSuccess();
      } catch {
        setError("Failed to save PIN. Please try again.");
      } finally {
        setLoading(false);
      }
      return;
    }

    // Verify mode
    setLoading(true);
    try {
      const ok = await verifyAccountPin(pin);
      if (ok) {
        onSuccess();
      } else {
        setError("Incorrect PIN. Please try again.");
      }
    } catch {
      setError("Could not verify PIN. Please try again.");
    } finally {
      setLoading(false);
    }
  }, [pin, confirmPin, isSetup, onSuccess]);

  return (
    <Modal
      visible={visible}
      transparent
      animationType="slide"
      onRequestClose={onCancel}
      accessibilityViewIsModal
    >
      <View style={pinStyles.overlay}>
        <View style={pinStyles.card}>
          <View style={pinStyles.header}>
            <Ionicons name="keypad-outline" size={28} color={COLORS.primary} />
            <Text style={pinStyles.title}>
              {isSetup ? "Set Up PIN" : "Enter PIN"}
            </Text>
          </View>

          <Text style={pinStyles.subtitle}>
            {isSetup
              ? "Create a PIN to secure your account when biometrics are unavailable."
              : "Enter your account PIN to authenticate."}
          </Text>

          <TextInput
            style={pinStyles.input}
            value={pin}
            onChangeText={(v) => {
              setPin(v);
              setError(null);
            }}
            placeholder={isSetup ? "Choose a PIN" : "Enter PIN"}
            placeholderTextColor="#BDBDBD"
            keyboardType="numeric"
            secureTextEntry
            maxLength={12}
            accessibilityLabel={isSetup ? "Choose a PIN" : "Enter PIN"}
            testID="pin-input"
          />

          {isSetup && (
            <TextInput
              style={[pinStyles.input, { marginTop: 10 }]}
              value={confirmPin}
              onChangeText={(v) => {
                setConfirmPin(v);
                setError(null);
              }}
              placeholder="Confirm PIN"
              placeholderTextColor="#BDBDBD"
              keyboardType="numeric"
              secureTextEntry
              maxLength={12}
              accessibilityLabel="Confirm PIN"
              testID="pin-confirm-input"
            />
          )}

          {error ? (
            <Text style={pinStyles.errorText} testID="pin-error">
              {error}
            </Text>
          ) : null}

          <View style={pinStyles.actions}>
            <TouchableOpacity
              style={pinStyles.cancelButton}
              onPress={onCancel}
              accessibilityRole="button"
              accessibilityLabel="Cancel"
              testID="pin-cancel-button"
            >
              <Text style={pinStyles.cancelButtonText}>Cancel</Text>
            </TouchableOpacity>

            <TouchableOpacity
              style={[
                pinStyles.submitButton,
                loading && pinStyles.submitButtonDisabled,
              ]}
              onPress={handleSubmit}
              disabled={loading}
              accessibilityRole="button"
              accessibilityLabel={isSetup ? "Save PIN" : "Verify PIN"}
              testID="pin-submit-button"
            >
              {loading ? (
                <ActivityIndicator color={COLORS.white} size="small" />
              ) : (
                <Text style={pinStyles.submitButtonText}>
                  {isSetup ? "Save PIN" : "Verify"}
                </Text>
              )}
            </TouchableOpacity>
          </View>
        </View>
      </View>
    </Modal>
  );
}

// ── authenticateWithBiometrics ────────────────────────────────────────────────

/**
 * Request biometric / device-credential authentication from the user.
 *
 * Falls back to the device passcode or PIN when biometric hardware is absent
 * or the user has no biometrics enrolled, by leaving `disableDeviceFallback`
 * as `false` (the expo-local-authentication default).
 *
 * @param promptMessage  Text shown in the system authentication dialog.
 *   Defaults to a generic payment-confirmation message.
 * @returns `BiometricAuthResult` – `{ success: true }` on success, or an
 *   object with `success: false` and a `reason` indicating why it failed.
 */
export async function authenticateWithBiometrics(
  promptMessage = "Verify your identity to proceed"
): Promise<BiometricAuthResult> {
  try {
    // 1. Hardware check ──────────────────────────────────────────────────────
    const hasHardware = await LocalAuthentication.hasHardwareAsync();
    if (!hasHardware) {
      // Device has no biometric sensor – fall back to device passcode only.
      const passcodeResult = await LocalAuthentication.authenticateAsync({
        promptMessage,
        // Allow device passcode when there is no biometric sensor.
        disableDeviceFallback: false,
        cancelLabel: "Cancel",
      });

      return passcodeResult.success
        ? { success: true }
        : {
            success: false,
            reason:
              passcodeResult.error === "user_cancel" ? "cancelled" : "failed",
          };
    }

    // 2. Enrollment check ────────────────────────────────────────────────────
    const isEnrolled = await LocalAuthentication.isEnrolledAsync();
    if (!isEnrolled) {
      // Hardware present but no biometrics enrolled; fall back to passcode.
      const passcodeResult = await LocalAuthentication.authenticateAsync({
        promptMessage,
        disableDeviceFallback: false,
        cancelLabel: "Cancel",
      });

      return passcodeResult.success
        ? { success: true }
        : {
            success: false,
            reason:
              passcodeResult.error === "user_cancel"
                ? "cancelled"
                : "not_enrolled",
          };
    }

    // 3. Biometric prompt ────────────────────────────────────────────────────
    const result = await LocalAuthentication.authenticateAsync({
      promptMessage,
      // Show a FaceID / Fingerprint prompt; automatically shows a fallback
      // "Use Passcode" button if biometrics fail (iOS) or the sensor is
      // temporarily unavailable (Android).
      disableDeviceFallback: false,
      cancelLabel: "Cancel",
      fallbackLabel: "Use Passcode",
    });

    if (result.success) {
      return { success: true };
    }

    return {
      success: false,
      reason: result.error === "user_cancel" ? "cancelled" : "failed",
    };
  } catch (err) {
    return {
      success: false,
      reason: "error",
      error: (err as Error)?.message ?? "Unknown error",
    };
  }
}

// ── BiometricScreen (onboarding) ──────────────────────────────────────────────

type HardwareStatus = "checking" | "available" | "unavailable" | "not_enrolled";

export default function BiometricScreen() {
  const router = useRouter();
  const { type } = useLocalSearchParams();
  const [loading, setLoading] = useState(false);
  const [hardwareStatus, setHardwareStatus] =
    useState<HardwareStatus>("checking");
  const [showPinModal, setShowPinModal] = useState(false);
  const [pinIsSetup, setPinIsSetup] = useState(false);

  // ── #684 — Check biometric hardware availability on mount ──────────────────
  useEffect(() => {
    let cancelled = false;

    async function checkHardware() {
      try {
        const hasHardware = await LocalAuthentication.hasHardwareAsync();

        if (!hasHardware) {
          if (!cancelled) setHardwareStatus("unavailable");
          return;
        }

        const isEnrolled = await LocalAuthentication.isEnrolledAsync();
        if (!cancelled) {
          setHardwareStatus(isEnrolled ? "available" : "not_enrolled");
        }
      } catch {
        if (!cancelled) setHardwareStatus("unavailable");
      }
    }

    checkHardware();
    return () => {
      cancelled = true;
    };
  }, []);

  // ── Auto-show PIN modal when biometrics are unavailable (#684) ──────────────
  useEffect(() => {
    if (
      hardwareStatus === "unavailable" ||
      hardwareStatus === "not_enrolled"
    ) {
      // Check whether a PIN has already been set up.
      AsyncStorage.getItem(PIN_HASH_KEY)
        .then((stored) => {
          setPinIsSetup(!stored);
          setShowPinModal(true);
        })
        .catch(() => {
          setPinIsSetup(true);
          setShowPinModal(true);
        });
    }
  }, [hardwareStatus]);

  const navigateForward = useCallback(() => {
    if (type === "returning") {
      router.replace("/account-type");
    } else {
      router.push("/create-wallet");
    }
  }, [type, router]);

  /**
   * Triggered when the user taps "Continue" on the onboarding screen.
   * Immediately attempts biometric enrolment verification so the user
   * confirms the sensor works before they rely on it later.
   *
   * If biometric hardware is unavailable or no biometrics are enrolled,
   * the PIN fallback modal is shown instead (#684).
   */
  const handleContinue = useCallback(async () => {
    // If hardware is already known to be unavailable, go straight to PIN.
    if (
      hardwareStatus === "unavailable" ||
      hardwareStatus === "not_enrolled"
    ) {
      setShowPinModal(true);
      return;
    }

    setLoading(true);
    try {
      const result = await authenticateWithBiometrics(
        "Confirm your identity to enable biometric login"
      );

      if (!result.success) {
        if (
          result.reason === "no_hardware" ||
          result.reason === "not_enrolled"
        ) {
          // Hardware unavailable — switch to PIN fallback smoothly (#684).
          setHardwareStatus("unavailable");
          AsyncStorage.getItem(PIN_HASH_KEY)
            .then((stored) => {
              setPinIsSetup(!stored);
              setShowPinModal(true);
            })
            .catch(() => {
              setPinIsSetup(true);
              setShowPinModal(true);
            });
          return;
        }

        if (result.reason === "failed" || result.reason === "error") {
          // Biometric failure — offer PIN fallback rather than hard-blocking (#684).
          Alert.alert(
            "Biometric Failed",
            "Biometric authentication failed. Would you like to use your PIN instead?",
            [
              {
                text: "Use PIN",
                onPress: () => {
                  AsyncStorage.getItem(PIN_HASH_KEY)
                    .then((stored) => {
                      setPinIsSetup(!stored);
                      setShowPinModal(true);
                    })
                    .catch(() => {
                      setPinIsSetup(true);
                      setShowPinModal(true);
                    });
                },
              },
              {
                text: "Try Again",
                style: "cancel",
              },
            ]
          );
          return;
        }

        // User cancelled — allow them to skip naturally.
      }
    } finally {
      setLoading(false);
    }

    navigateForward();
  }, [hardwareStatus, navigateForward]);

  const handleSkip = useCallback(() => {
    navigateForward();
  }, [navigateForward]);

  // ── Render fallback banner when hardware is unavailable ───────────────────

  const renderHardwareBanner = () => {
    if (hardwareStatus === "checking") {
      return (
        <View style={styles.hardwareBanner} testID="hardware-checking-banner">
          <ActivityIndicator size="small" color={COLORS.primary} />
          <Text style={styles.hardwareBannerText}>
            Checking biometric availability…
          </Text>
        </View>
      );
    }

    if (hardwareStatus === "unavailable") {
      return (
        <View
          style={[styles.hardwareBanner, styles.hardwareBannerWarn]}
          testID="hardware-unavailable-banner"
        >
          <Ionicons
            name="warning-outline"
            size={18}
            color={COLORS.primary}
          />
          <Text style={styles.hardwareBannerText}>
            Biometric hardware not available. You can use a PIN instead.
          </Text>
        </View>
      );
    }

    if (hardwareStatus === "not_enrolled") {
      return (
        <View
          style={[styles.hardwareBanner, styles.hardwareBannerWarn]}
          testID="hardware-not-enrolled-banner"
        >
          <Ionicons
            name="finger-print-outline"
            size={18}
            color={COLORS.primary}
          />
          <Text style={styles.hardwareBannerText}>
            No biometrics enrolled on this device. You can use a PIN instead.
          </Text>
        </View>
      );
    }

    return null;
  };

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      <View style={styles.header}>
        <TouchableOpacity
          style={styles.backButton}
          onPress={() => router.back()}
          accessibilityRole="button"
          accessibilityLabel="Go back"
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Secure Your Account</Text>
        <View style={{ width: 24 }} />
      </View>

      <View style={styles.content}>
        <View style={styles.iconContainer}>
          <Image
            source={require("../assets/fingerprint.png")}
            style={styles.fingerprint}
            resizeMode="contain"
          />
        </View>

        <View style={styles.textContainer}>
          <Text style={styles.title}>
            {hardwareStatus === "unavailable" ||
            hardwareStatus === "not_enrolled"
              ? "Secure with PIN"
              : "Enable Biometric Login"}
          </Text>
          <Text style={styles.subtitle}>
            {hardwareStatus === "unavailable" ||
            hardwareStatus === "not_enrolled"
              ? "Your device doesn't support biometrics. Set up a PIN to protect your wallet."
              : "Use your fingerprint or face to quickly and securely access your wallet"}
          </Text>
        </View>

        {renderHardwareBanner()}
      </View>

      <View style={styles.footer}>
        <Button
          title={
            loading
              ? "Verifying…"
              : hardwareStatus === "unavailable" ||
                  hardwareStatus === "not_enrolled"
                ? "Set Up PIN"
                : "Continue"
          }
          onPress={handleContinue}
          variant="primary"
          style={styles.mainButton}
          disabled={loading || hardwareStatus === "checking"}
        />
        <Button
          title="Skip for now"
          onPress={handleSkip}
          variant="secondary"
          style={styles.skipButton}
          textStyle={{ color: COLORS.primary }}
          disabled={loading}
        />
      </View>

      {/* #684 — PIN Fallback Modal */}
      <PinFallbackModal
        visible={showPinModal}
        isSetup={pinIsSetup}
        onSuccess={() => {
          setShowPinModal(false);
          navigateForward();
        }}
        onCancel={() => setShowPinModal(false)}
      />
    </SafeAreaView>
  );
}

// ── Styles ────────────────────────────────────────────────────────────────────

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: COLORS.white,
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 20,
    paddingVertical: 10,
  },
  backButton: {
    padding: 8,
  },
  headerTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  content: {
    flex: 1,
    paddingHorizontal: 20,
    justifyContent: "center",
    alignItems: "center",
    paddingBottom: 100,
  },
  iconContainer: {
    width: 120,
    height: 120,
    borderRadius: 60,
    backgroundColor: "transparent",
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 40,
    borderWidth: 1,
    borderColor: "#eee",
  },
  fingerprint: {
    width: 60,
    height: 60,
    tintColor: COLORS.primary,
  },
  textContainer: {
    alignItems: "center",
    marginBottom: 16,
  },
  title: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 12,
    textAlign: "center",
  },
  subtitle: {
    fontSize: 16,
    color: "#666",
    textAlign: "center",
    lineHeight: 24,
    maxWidth: "80%",
    fontFamily: "Outfit_500Medium",
  },
  hardwareBanner: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    backgroundColor: "#F0FFF4",
    borderRadius: 10,
    paddingHorizontal: 14,
    paddingVertical: 10,
    marginTop: 16,
    width: "100%",
  },
  hardwareBannerWarn: {
    backgroundColor: "#FFFBEB",
  },
  hardwareBannerText: {
    flex: 1,
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: COLORS.darkGray,
    lineHeight: 18,
  },
  footer: {
    padding: 20,
    paddingBottom: 40,
    gap: 12,
  },
  mainButton: {
    marginBottom: 0,
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    height: 60,
  },
  skipButton: {
    backgroundColor: COLORS.secondary,
    borderRadius: 100,
    height: 60,
  },
});

const pinStyles = StyleSheet.create({
  overlay: {
    flex: 1,
    backgroundColor: "rgba(0, 0, 0, 0.55)",
    justifyContent: "flex-end",
    paddingHorizontal: 0,
  },
  card: {
    backgroundColor: COLORS.white,
    borderTopLeftRadius: 24,
    borderTopRightRadius: 24,
    padding: 28,
    paddingBottom: 48,
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
    marginBottom: 10,
  },
  title: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  subtitle: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    lineHeight: 20,
    marginBottom: 18,
  },
  input: {
    borderWidth: 1,
    borderColor: "#E0E0E0",
    borderRadius: 12,
    paddingHorizontal: 16,
    paddingVertical: 14,
    fontSize: 18,
    fontFamily: "Outfit_400Regular",
    color: COLORS.black,
    letterSpacing: 4,
  },
  errorText: {
    color: "#DC2626",
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    marginTop: 8,
  },
  actions: {
    flexDirection: "row",
    gap: 12,
    marginTop: 20,
  },
  cancelButton: {
    flex: 1,
    height: 52,
    borderRadius: 26,
    justifyContent: "center",
    alignItems: "center",
    backgroundColor: COLORS.gray,
  },
  cancelButtonText: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: COLORS.darkGray,
  },
  submitButton: {
    flex: 2,
    height: 52,
    borderRadius: 26,
    justifyContent: "center",
    alignItems: "center",
    backgroundColor: COLORS.primary,
  },
  submitButtonDisabled: {
    opacity: 0.6,
  },
  submitButtonText: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: COLORS.white,
  },
});
