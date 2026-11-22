import React, { useState } from "react";
import { View, StyleSheet, Pressable, ActivityIndicator } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { Feather } from "@expo/vector-icons";
import * as Haptics from "expo-haptics";
import { useLocalSearchParams, useRouter } from "expo-router";

import { ThemedText } from "@/src/components/ThemedText";
import { useTheme } from "@/src/hooks/useTheme";
import { Spacing, BorderRadius } from "@/src/constants/theme";
import { submitWithdrawal, WithdrawalError } from "@/src/services/withdrawalService";

export default function TransferConfirmationScreen() {
  const insets = useSafeAreaInsets();
  const { theme } = useTheme();
  const router = useRouter();
  const params = useLocalSearchParams<{
    amount?: string;
    accountName?: string;
    accountNumberLast4?: string;
    bankCode?: string;
    bankName?: string;
    destinationId?: string;
  }>();
  const amount = Number(params.amount);
  const accountName = params.accountName ?? "";
  const accountNumberLast4 = params.accountNumberLast4 ?? "";
  const bankCode = params.bankCode ?? "";
  const bankName = params.bankName ?? "";
  const destinationId = params.destinationId ?? "";
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  const valid =
    Number.isFinite(amount) &&
    amount > 0 &&
    !!accountNumberLast4 &&
    !!bankCode &&
    !!bankName &&
    !submitting;

  const confirmWithdrawal = async () => {
    if (!valid) return;
    setSubmitting(true);
    setError("");
    try {
      const result = await submitWithdrawal({
        amount,
        destination: {
          id: destinationId || undefined,
          bankCode,
          accountName,
          accountNumberLast4,
          bankName,
        },
      });
      if (!result.accepted) throw new Error("Withdrawal was not accepted.");
      await Haptics.notificationAsync(
        Haptics.NotificationFeedbackType.Success
      );
      const successParams = new URLSearchParams({
        amount: amount.toFixed(2),
        requestId: result.id,
        accepted: "true",
      });
      router.replace(`/merchant/success?${successParams.toString()}`);
    } catch (e) {
      await Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error);
      setError(
        e instanceof WithdrawalError
          ? e.message
          : e instanceof Error
            ? e.message
            : "Withdrawal failed. Please try again."
      );
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <View style={[styles.container, { backgroundColor: theme.backgroundRoot }]}>
      <View
        style={[
          styles.content,
          {
            paddingTop: insets.top + Spacing["4xl"],
            paddingBottom: insets.bottom + Spacing["2xl"],
          },
        ]}
      >
        <View style={styles.header}>
          <View style={[styles.lockCircle, { backgroundColor: theme.primary + "15" }]}>
            <Feather name="check-circle" size={32} color={theme.primary} />
          </View>
          <ThemedText style={[styles.title, { color: theme.text }]}>
            Confirm withdrawal
          </ThemedText>
          <ThemedText style={[styles.subtitle, { color: theme.textSecondary }]}>
            Review and confirm to submit your withdrawal.
          </ThemedText>
        </View>

        <View style={[styles.details, { borderColor: theme.border }]}>
          <View style={styles.detailRow}>
            <ThemedText style={{ color: theme.textSecondary }}>Amount</ThemedText>
            <ThemedText style={{ color: theme.text }}>
              ${Number.isFinite(amount) ? amount.toFixed(2) : "0.00"}
            </ThemedText>
          </View>
          <View style={styles.detailRow}>
            <ThemedText style={{ color: theme.textSecondary }}>Bank</ThemedText>
            <ThemedText style={{ color: theme.text }}>{bankName || "Missing"}</ThemedText>
          </View>
          <View style={styles.detailRow}>
            <ThemedText style={{ color: theme.textSecondary }}>Account</ThemedText>
            <ThemedText style={{ color: theme.text }}>
              {accountNumberLast4 ? `•••• ${accountNumberLast4}` : "Missing"}
            </ThemedText>
          </View>
        </View>

        {error ? (
          <View style={styles.errorContainer} testID="withdrawal-submit-error">
            <Feather name="alert-circle" size={16} color="#EF4444" />
            <ThemedText style={styles.errorText}>{error}</ThemedText>
          </View>
        ) : null}

        <Pressable
          onPress={confirmWithdrawal}
          disabled={!valid}
          testID="submit-withdrawal"
          style={({ pressed }) => [
            styles.confirmButton,
            {
              backgroundColor: theme.primary,
              opacity: !valid ? 0.5 : pressed ? 0.8 : 1,
            },
          ]}
        >
          {submitting ? (
            <ActivityIndicator color="#FFFFFF" />
          ) : (
            <ThemedText style={styles.confirmText}>Confirm & withdraw</ThemedText>
          )}
        </Pressable>

        <Pressable
          onPress={() => router.back()}
          disabled={submitting}
          style={({ pressed }) => [
            styles.cancelButton,
            { opacity: submitting ? 0.4 : pressed ? 0.7 : 1 },
          ]}
        >
          <ThemedText style={[styles.cancelText, { color: theme.textSecondary }]}>
            Cancel
          </ThemedText>
        </Pressable>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1 },
  content: {
    flex: 1,
    paddingHorizontal: Spacing.lg,
    justifyContent: "center",
  },
  header: { alignItems: "center", marginBottom: Spacing["3xl"] },
  lockCircle: {
    width: 80,
    height: 80,
    borderRadius: 40,
    alignItems: "center",
    justifyContent: "center",
    marginBottom: Spacing.xl,
  },
  title: { fontSize: 24, fontFamily: "Outfit_700Bold", marginBottom: Spacing.sm },
  subtitle: { fontSize: 14, textAlign: "center" },
  details: {
    borderWidth: 1,
    borderRadius: BorderRadius.xl,
    padding: Spacing["2xl"],
    gap: Spacing.lg,
  },
  detailRow: { flexDirection: "row", justifyContent: "space-between", gap: Spacing.lg },
  errorContainer: {
    flexDirection: "row",
    alignItems: "center",
    gap: Spacing.xs,
    marginTop: Spacing.lg,
  },
  errorText: { flex: 1, fontSize: 14, color: "#EF4444", fontFamily: "Outfit_500Medium" },
  confirmButton: {
    height: Spacing.buttonHeight,
    borderRadius: BorderRadius.full,
    alignItems: "center",
    justifyContent: "center",
    marginTop: Spacing["3xl"],
  },
  confirmText: { color: "#FFFFFF", fontSize: 16, fontFamily: "Outfit_600SemiBold" },
  cancelButton: { marginTop: Spacing.lg, alignItems: "center", padding: Spacing.md },
  cancelText: { fontSize: 16, fontFamily: "Outfit_500Medium" },
});
