/**
 * claim/[token].tsx
 *
 * SDP Payout Claim Validation Screen — Issue #703
 *
 * Deep Link Handler for `zaps://claim?token=<SDP_TOKEN>`:
 *  - The root layout (_layout.tsx) handles `Linking.getInitialURL()` (cold
 *    start) and the `url` event (warm start) via `parseSdpClaimUrl`, then
 *    navigates to `/claim/<token>`.
 *  - This screen receives the token as a route param via `useLocalSearchParams`
 *    so the claim token input is **auto-populated** — the user never needs to
 *    copy-paste the token manually.
 *  - The token value is also displayed to the user for transparency so they
 *    can verify the link before anything is submitted.
 *
 * Claim validation flow:
 *   1. Screen mounts → reads `token` from route params (pre-populated).
 *   2. Automatically calls `validateClaimToken(token)`.
 *   3. Renders a result card based on the validation status.
 *   4. User can retry or navigate to home.
 */

import React, { useEffect, useState, useCallback } from "react";
import {
  View,
  Text,
  StyleSheet,
  ActivityIndicator,
  TouchableOpacity,
  TextInput,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useLocalSearchParams, useRouter, Stack } from "expo-router";
import { COLORS } from "../../src/constants/colors";
import {
  validateClaimToken,
  ClaimValidationResult,
} from "../../src/services/sdpService";
import { isValidSdpToken } from "../../src/utils/sdpDeepLink";

type ScreenState = "validating" | "result" | "error" | "invalid_token";

export default function ClaimValidationScreen() {
  // `token` is injected by expo-router from the dynamic route segment
  // `/claim/[token]` — this is the value extracted from the deep link URL
  // by parseSdpClaimUrl() in _layout.tsx and passed here automatically.
  const { token: routeToken } = useLocalSearchParams<{ token: string }>();
  const router = useRouter();

  // The pre-populated token field mirrors the route param. Users may also
  // manually edit this field if they received the token via a different channel.
  const [tokenInput, setTokenInput] = useState<string>(routeToken ?? "");
  const [screenState, setScreenState] = useState<ScreenState>(
    routeToken ? "validating" : "invalid_token"
  );
  const [result, setResult] = useState<ClaimValidationResult | null>(null);
  const [tokenError, setTokenError] = useState<string | null>(null);

  // Auto-populate when the route param changes (e.g. app resumed with a new link).
  useEffect(() => {
    if (routeToken && routeToken !== tokenInput) {
      setTokenInput(routeToken);
      setTokenError(null);
    }
  }, [routeToken]); // eslint-disable-line react-hooks/exhaustive-deps

  const runValidation = useCallback(
    async (token?: string) => {
      const effectiveToken = (token ?? tokenInput ?? "").trim();

      if (!effectiveToken) {
        setTokenError("Please enter a claim token.");
        setScreenState("invalid_token");
        return;
      }

      if (!isValidSdpToken(effectiveToken)) {
        setTokenError(
          "The token format is invalid. Please check the link and try again."
        );
        setScreenState("invalid_token");
        return;
      }

      setTokenError(null);
      setScreenState("validating");

      try {
        const validation = await validateClaimToken(effectiveToken);
        setResult(validation);
        setScreenState("result");
      } catch {
        setScreenState("error");
      }
    },
    [tokenInput]
  );

  // Auto-validate on mount when a token was provided via the deep link.
  useEffect(() => {
    if (routeToken) {
      runValidation(routeToken);
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const renderBody = () => {
    if (screenState === "validating") {
      return (
        <View style={styles.centered}>
          <ActivityIndicator size="large" color={COLORS.primary} />
          <Text style={styles.statusText}>Validating your claim…</Text>
        </View>
      );
    }

    if (screenState === "invalid_token") {
      return (
        <View style={styles.centered}>
          <Ionicons
            name="alert-circle-outline"
            size={56}
            color={COLORS.primary}
          />
          <Text style={styles.title}>Enter Your Claim Token</Text>
          <Text style={styles.subtitle}>
            Your claim link should have been sent via SMS or email. Paste the
            token below, or open the link directly on your device.
          </Text>

          {/* ── Pre-populated token input field (#703) ─────────────────── */}
          <View style={styles.tokenInputWrapper}>
            <TextInput
              style={[
                styles.tokenInput,
                tokenError ? styles.tokenInputError : null,
              ]}
              value={tokenInput}
              onChangeText={(text) => {
                setTokenInput(text);
                setTokenError(null);
              }}
              placeholder="Paste your claim token here"
              placeholderTextColor="#BDBDBD"
              autoCapitalize="none"
              autoCorrect={false}
              accessibilityLabel="Claim token input"
              testID="claim-token-input"
            />
            {tokenError ? (
              <Text style={styles.tokenErrorText} testID="claim-token-error">
                {tokenError}
              </Text>
            ) : null}
          </View>

          <TouchableOpacity
            style={styles.primaryButton}
            onPress={() => runValidation()}
            testID="claim-validate-button"
          >
            <Text style={styles.primaryButtonText}>Validate Claim</Text>
          </TouchableOpacity>
        </View>
      );
    }

    if (screenState === "error" || !result) {
      return (
        <View style={styles.centered}>
          <Ionicons name="alert-circle-outline" size={56} color="#EF4444" />
          <Text style={styles.title}>Something went wrong</Text>
          <Text style={styles.subtitle}>
            We couldn&apos;t validate this claim link. Please try opening it
            again.
          </Text>

          {/* Show the token that was attempted — aids debugging */}
          {tokenInput ? (
            <View style={styles.tokenDisplay} testID="claim-token-display">
              <Text style={styles.tokenDisplayLabel}>Token:</Text>
              <Text
                style={styles.tokenDisplayValue}
                numberOfLines={1}
                ellipsizeMode="middle"
              >
                {tokenInput}
              </Text>
            </View>
          ) : null}

          <TouchableOpacity
            style={styles.primaryButton}
            onPress={() => runValidation()}
          >
            <Text style={styles.primaryButtonText}>Retry</Text>
          </TouchableOpacity>
        </View>
      );
    }

    switch (result.status) {
      case "valid":
        return (
          <View style={styles.centered}>
            <Ionicons
              name="checkmark-circle-outline"
              size={56}
              color="#22C55E"
            />
            <Text style={styles.title}>Funds Waiting for You</Text>
            <Text style={styles.subtitle}>
              {result.amount
                ? `${result.amount} ${result.assetCode ?? ""} is ready to be claimed.`
                : "Your disbursement is ready to be claimed."}
            </Text>

            {/* ── Token display: surface the auto-populated token (#703) ── */}
            {tokenInput ? (
              <View style={styles.tokenDisplay} testID="claim-token-display">
                <Text style={styles.tokenDisplayLabel}>Claim token:</Text>
                <Text
                  style={styles.tokenDisplayValue}
                  numberOfLines={1}
                  ellipsizeMode="middle"
                  testID="claim-token-value"
                >
                  {tokenInput}
                </Text>
              </View>
            ) : null}

            <TouchableOpacity
              style={styles.primaryButton}
              onPress={() => router.replace("/(personal)/home")}
              testID="claim-continue-button"
            >
              <Text style={styles.primaryButtonText}>Continue</Text>
            </TouchableOpacity>
          </View>
        );

      case "already_claimed":
        return (
          <View style={styles.centered}>
            <Ionicons
              name="information-circle-outline"
              size={56}
              color="#F59E0B"
            />
            <Text style={styles.title}>Already Claimed</Text>
            <Text style={styles.subtitle}>
              This disbursement has already been claimed.
            </Text>
          </View>
        );

      case "expired":
        return (
          <View style={styles.centered}>
            <Ionicons name="time-outline" size={56} color="#F59E0B" />
            <Text style={styles.title}>Link Expired</Text>
            <Text style={styles.subtitle}>
              This claim link has expired. Please contact the sender for a new
              invite.
            </Text>
          </View>
        );

      case "invalid":
      default:
        return (
          <View style={styles.centered}>
            <Ionicons name="close-circle-outline" size={56} color="#EF4444" />
            <Text style={styles.title}>Invalid Claim Link</Text>
            <Text style={styles.subtitle}>
              This claim link isn&apos;t recognized. Double-check the link and
              try again.
            </Text>
          </View>
        );
    }
  };

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />
      <View style={styles.header}>
        <TouchableOpacity
          onPress={() => router.back()}
          style={styles.backButton}
          accessibilityRole="button"
          accessibilityLabel="Go back"
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Claim Disbursement</Text>
        <View style={{ width: 24 }} />
      </View>
      {renderBody()}
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
    paddingVertical: 15,
  },
  backButton: {
    padding: 5,
  },
  headerTitle: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  centered: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 32,
    gap: 12,
  },
  statusText: {
    fontSize: 15,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    marginTop: 8,
  },
  title: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    textAlign: "center",
    marginTop: 8,
  },
  subtitle: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    textAlign: "center",
    lineHeight: 20,
  },
  primaryButton: {
    backgroundColor: COLORS.primary,
    height: 52,
    borderRadius: 26,
    justifyContent: "center",
    alignItems: "center",
    paddingHorizontal: 32,
    marginTop: 12,
  },
  primaryButtonText: {
    color: COLORS.white,
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
  },
  // Token input (manual entry fallback)
  tokenInputWrapper: {
    width: "100%",
    marginVertical: 8,
  },
  tokenInput: {
    borderWidth: 1,
    borderColor: "#E0E0E0",
    borderRadius: 12,
    paddingHorizontal: 16,
    paddingVertical: 14,
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: COLORS.black,
    width: "100%",
  },
  tokenInputError: {
    borderColor: "#DC2626",
  },
  tokenErrorText: {
    color: "#DC2626",
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    marginTop: 4,
  },
  // Token display (read-only, shows the auto-populated value)
  tokenDisplay: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    backgroundColor: "#F0F9FF",
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
    width: "100%",
    marginTop: 4,
  },
  tokenDisplayLabel: {
    fontSize: 12,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
    flexShrink: 0,
  },
  tokenDisplayValue: {
    flex: 1,
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: COLORS.darkGray,
  },
});
