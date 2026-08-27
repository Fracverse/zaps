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
 */

import React, { useCallback, useState } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  Image,
  Alert,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Stack, useRouter, useLocalSearchParams } from "expo-router";
import * as LocalAuthentication from "expo-local-authentication";
import { COLORS } from "../src/constants/colors";
import { Button } from "../src/components/Button";
import { Ionicons } from "@expo/vector-icons";

// ── Biometric helper types ────────────────────────────────────────────────────

export type BiometricAuthResult =
  | { success: true }
  | { success: false; reason: "no_hardware" | "not_enrolled" | "cancelled" | "failed" | "error"; error?: string };

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
            reason: passcodeResult.error === "user_cancel" ? "cancelled" : "failed",
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
            reason: passcodeResult.error === "user_cancel" ? "cancelled" : "not_enrolled",
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

export default function BiometricScreen() {
  const router = useRouter();
  const { type } = useLocalSearchParams();
  const [loading, setLoading] = useState(false);

  /**
   * Triggered when the user taps "Continue" on the onboarding screen.
   * Immediately attempts biometric enrolment verification so the user
   * confirms the sensor works before they rely on it later.
   */
  const handleContinue = useCallback(async () => {
    setLoading(true);
    try {
      const result = await authenticateWithBiometrics(
        "Confirm your identity to enable biometric login"
      );

      if (!result.success) {
        if (result.reason === "no_hardware" || result.reason === "not_enrolled") {
          Alert.alert(
            "Biometric Unavailable",
            "Your device does not have biometric hardware set up. You can still use your device passcode instead.",
            [{ text: "OK" }]
          );
        } else if (result.reason !== "cancelled") {
          Alert.alert(
            "Authentication Failed",
            "Could not verify your identity. Please try again.",
            [{ text: "OK" }]
          );
          return;
        } else {
          // User cancelled — allow them to skip naturally.
        }
      }
    } finally {
      setLoading(false);
    }

    if (type === "returning") {
      router.replace("/account-type");
    } else {
      router.push("/create-wallet");
    }
  }, [type, router]);

  const handleSkip = useCallback(() => {
    if (type === "returning") {
      router.replace("/account-type");
    } else {
      router.push("/create-wallet");
    }
  }, [type, router]);

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      <View style={styles.header}>
        <TouchableOpacity
          style={styles.backButton}
          onPress={() => router.back()}
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
          <Text style={styles.title}>Enable Biometric Login</Text>
          <Text style={styles.subtitle}>
            Use your fingerprint or face to quickly and securely access your
            wallet
          </Text>
        </View>
      </View>

      <View style={styles.footer}>
        <Button
          title={loading ? "Verifying…" : "Continue"}
          onPress={handleContinue}
          variant="primary"
          style={styles.mainButton}
          disabled={loading}
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
    </SafeAreaView>
  );
}

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
    paddingBottom: 100, // Visual balance
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
  },
  title: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 12,
  },
  subtitle: {
    fontSize: 16,
    color: "#666",
    textAlign: "center",
    lineHeight: 24,
    maxWidth: "80%",
    fontFamily: "Outfit_500Medium",
  },
  footer: {
    padding: 20,
    paddingBottom: 40,
    gap: 12,
  },
  mainButton: {
    marginBottom: 0,
    backgroundColor: "#1A4B4A",
    borderRadius: 100,
    height: 60,
  },
  skipButton: {
    backgroundColor: COLORS.secondary,
    borderRadius: 100,
    height: 60,
  },
});
