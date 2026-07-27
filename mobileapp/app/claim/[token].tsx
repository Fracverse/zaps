import React, { useEffect, useState, useCallback } from "react";
import {
  View,
  Text,
  StyleSheet,
  ActivityIndicator,
  TouchableOpacity,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useLocalSearchParams, useRouter, Stack } from "expo-router";
import { COLORS } from "../../src/constants/colors";
import {
  validateClaimToken,
  ClaimValidationResult,
} from "../../src/services/sdpService";

type ScreenState = "validating" | "result" | "error";

export default function ClaimValidationScreen() {
  const { token } = useLocalSearchParams<{ token: string }>();
  const router = useRouter();
  const [screenState, setScreenState] = useState<ScreenState>("validating");
  const [result, setResult] = useState<ClaimValidationResult | null>(null);

  const runValidation = useCallback(async () => {
    if (!token) {
      setScreenState("error");
      return;
    }
    setScreenState("validating");
    try {
      const validation = await validateClaimToken(token);
      setResult(validation);
      setScreenState("result");
    } catch {
      setScreenState("error");
    }
  }, [token]);

  useEffect(() => {
    runValidation();
  }, [runValidation]);

  const renderBody = () => {
    if (screenState === "validating") {
      return (
        <View style={styles.centered}>
          <ActivityIndicator size="large" color={COLORS.primary} />
          <Text style={styles.statusText}>Validating your claim…</Text>
        </View>
      );
    }

    if (screenState === "error" || !result) {
      return (
        <View style={styles.centered}>
          <Ionicons name="alert-circle-outline" size={56} color="#EF4444" />
          <Text style={styles.title}>Something went wrong</Text>
          <Text style={styles.subtitle}>
            We couldn't validate this claim link. Please try opening it again.
          </Text>
          <TouchableOpacity style={styles.primaryButton} onPress={runValidation}>
            <Text style={styles.primaryButtonText}>Retry</Text>
          </TouchableOpacity>
        </View>
      );
    }

    switch (result.status) {
      case "valid":
        return (
          <View style={styles.centered}>
            <Ionicons name="checkmark-circle-outline" size={56} color="#22C55E" />
            <Text style={styles.title}>Funds Waiting for You</Text>
            <Text style={styles.subtitle}>
              {result.amount
                ? `${result.amount} ${result.assetCode ?? ""} is ready to be claimed.`
                : "Your disbursement is ready to be claimed."}
            </Text>
            <TouchableOpacity
              style={styles.primaryButton}
              onPress={() => router.replace("/(personal)/home")}
            >
              <Text style={styles.primaryButtonText}>Continue</Text>
            </TouchableOpacity>
          </View>
        );
      case "already_claimed":
        return (
          <View style={styles.centered}>
            <Ionicons name="information-circle-outline" size={56} color="#F59E0B" />
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
              This claim link has expired. Please contact the sender for a new invite.
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
              This claim link isn't recognized. Double-check the link and try again.
            </Text>
          </View>
        );
    }
  };

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />
      <View style={styles.header}>
        <TouchableOpacity onPress={() => router.back()} style={styles.backButton}>
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
});
