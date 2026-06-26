import React, { useState } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  TextInput,
  Modal,
  KeyboardAvoidingView,
  Platform,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useRouter, Stack } from "expo-router";
import { COLORS } from "../src/constants/colors";

// ── Types ────────────────────────────────────────────────────────────────────

type TransactionType = "deposit" | "withdraw";

const NAIRA_KEYPAD_KEYS = [
  "1", "2", "3",
  "4", "5", "6",
  "7", "8", "9",
  ".", "0", "⌫",
];

// ── Naira Keypad ─────────────────────────────────────────────────────────────

function NairaKeypad({
  onPress,
}: {
  onPress: (key: string) => void;
}) {
  return (
    <View style={styles.keypad}>
      {NAIRA_KEYPAD_KEYS.map((key) => (
        <TouchableOpacity
          key={key}
          style={styles.keypadKey}
          onPress={() => onPress(key)}
          activeOpacity={0.7}
        >
          <Text style={styles.keypadKeyText}>{key}</Text>
        </TouchableOpacity>
      ))}
    </View>
  );
}

// ── Main Screen ──────────────────────────────────────────────────────────────

export default function YieldTransactionScreen() {
  const router = useRouter();
  const [mode, setMode] = useState<TransactionType>("deposit");
  const [amount, setAmount] = useState("");
  const [showConfirm, setShowConfirm] = useState(false);
  const [submitted, setSubmitted] = useState(false);

  // Simulated balances — replace with API/store values in production
  const availableBalance = 150_000;
  const yieldBalance = 23_480;

  const activeBalance = mode === "deposit" ? availableBalance : yieldBalance;
  const balanceLabel =
    mode === "deposit" ? "Available Balance" : "Earning Balance";

  const numericAmount = parseFloat(amount) || 0;
  const exceedsLimit = numericAmount > activeBalance;
  const isValid = numericAmount > 0 && !exceedsLimit;

  function handleKeyPress(key: string) {
    if (key === "⌫") {
      setAmount((prev) => prev.slice(0, -1));
      return;
    }
    if (key === "." && amount.includes(".")) return;
    if (amount === "0" && key !== ".") {
      setAmount(key);
      return;
    }
    // Cap at 2 decimal places
    const parts = (amount + key).split(".");
    if (parts[1] && parts[1].length > 2) return;
    setAmount((prev) => prev + key);
  }

  function handleConfirm() {
    setShowConfirm(false);
    setSubmitted(true);
  }

  if (submitted) {
    return (
      <SafeAreaView style={styles.container}>
        <Stack.Screen options={{ headerShown: false }} />
        <View style={styles.successContainer}>
          <View style={styles.successCircle}>
            <Ionicons name="checkmark" size={56} color={COLORS.primary} />
          </View>
          <Text style={styles.successTitle}>
            {mode === "deposit" ? "Deposit Successful!" : "Withdrawal Successful!"}
          </Text>
          <Text style={styles.successSubtitle}>
            ₦{parseFloat(amount).toLocaleString("en-NG", { minimumFractionDigits: 2 })}{" "}
            {mode === "deposit"
              ? "has been moved to your earning balance."
              : "has been returned to your available balance."}
          </Text>
          <TouchableOpacity
            style={styles.doneButton}
            onPress={() => router.replace("/(personal)/home")}
          >
            <Text style={styles.doneButtonText}>Back to Home</Text>
          </TouchableOpacity>
        </View>
      </SafeAreaView>
    );
  }

  return (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      {/* Header */}
      <View style={styles.header}>
        <TouchableOpacity onPress={() => router.back()} style={styles.backButton}>
          <Ionicons name="arrow-back" size={24} color={COLORS.primary} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Yield Transaction</Text>
        <View style={{ width: 40 }} />
      </View>

      <KeyboardAvoidingView
        style={{ flex: 1 }}
        behavior={Platform.OS === "ios" ? "padding" : undefined}
      >
        <ScrollView
          contentContainerStyle={styles.scrollContent}
          keyboardShouldPersistTaps="handled"
          showsVerticalScrollIndicator={false}
        >
          {/* Mode toggle */}
          <View style={styles.toggleRow}>
            <TouchableOpacity
              style={[styles.toggleBtn, mode === "deposit" && styles.toggleBtnActive]}
              onPress={() => { setMode("deposit"); setAmount(""); }}
            >
              <Ionicons
                name="arrow-down-circle-outline"
                size={18}
                color={mode === "deposit" ? COLORS.secondary : COLORS.primary}
              />
              <Text style={[styles.toggleBtnText, mode === "deposit" && styles.toggleBtnTextActive]}>
                Deposit
              </Text>
            </TouchableOpacity>

            <TouchableOpacity
              style={[styles.toggleBtn, mode === "withdraw" && styles.toggleBtnActive]}
              onPress={() => { setMode("withdraw"); setAmount(""); }}
            >
              <Ionicons
                name="arrow-up-circle-outline"
                size={18}
                color={mode === "withdraw" ? COLORS.secondary : COLORS.primary}
              />
              <Text style={[styles.toggleBtnText, mode === "withdraw" && styles.toggleBtnTextActive]}>
                Withdraw
              </Text>
            </TouchableOpacity>
          </View>

          {/* Balance display */}
          <View style={styles.balanceCard}>
            <Text style={styles.balanceLabel}>{balanceLabel}</Text>
            <Text style={styles.balanceAmount}>
              ₦{activeBalance.toLocaleString("en-NG")}
            </Text>
          </View>

          {/* Description */}
          <Text style={styles.description}>
            {mode === "deposit"
              ? "Move funds from your available balance into your yield-earning balance to start earning interest."
              : "Move funds from your yield-earning balance back to your available balance."}
          </Text>

          {/* Amount display */}
          <View style={[styles.amountDisplay, exceedsLimit && styles.amountDisplayError]}>
            <Text style={styles.amountCurrency}>₦</Text>
            <Text style={[styles.amountValue, !amount && styles.amountPlaceholder]}>
              {amount || "0.00"}
            </Text>
          </View>

          {exceedsLimit && (
            <Text style={styles.limitError}>
              Amount exceeds your {balanceLabel.toLowerCase()} of ₦
              {activeBalance.toLocaleString("en-NG")}
            </Text>
          )}

          {/* Quick-fill buttons */}
          <View style={styles.quickFillRow}>
            {[25, 50, 75, 100].map((pct) => (
              <TouchableOpacity
                key={pct}
                style={styles.quickFillBtn}
                onPress={() =>
                  setAmount(((activeBalance * pct) / 100).toFixed(2))
                }
              >
                <Text style={styles.quickFillText}>{pct}%</Text>
              </TouchableOpacity>
            ))}
          </View>

          {/* Naira keypad */}
          <NairaKeypad onPress={handleKeyPress} />

          {/* Confirm button */}
          <TouchableOpacity
            style={[styles.confirmButton, !isValid && styles.confirmButtonDisabled]}
            onPress={() => isValid && setShowConfirm(true)}
            disabled={!isValid}
          >
            <Text style={styles.confirmButtonText}>
              {mode === "deposit" ? "Deposit to Yield" : "Withdraw from Yield"}
            </Text>
          </TouchableOpacity>
        </ScrollView>
      </KeyboardAvoidingView>

      {/* Confirmation modal */}
      <Modal visible={showConfirm} transparent animationType="slide">
        <View style={styles.modalOverlay}>
          <View style={styles.modalCard}>
            <Text style={styles.modalTitle}>Confirm{" "}
              {mode === "deposit" ? "Deposit" : "Withdrawal"}
            </Text>

            <View style={styles.modalRow}>
              <Text style={styles.modalLabel}>Amount</Text>
              <Text style={styles.modalValue}>
                ₦{parseFloat(amount).toLocaleString("en-NG", { minimumFractionDigits: 2 })}
              </Text>
            </View>
            <View style={styles.modalRow}>
              <Text style={styles.modalLabel}>From</Text>
              <Text style={styles.modalValue}>
                {mode === "deposit" ? "Available Balance" : "Earning Balance"}
              </Text>
            </View>
            <View style={styles.modalRow}>
              <Text style={styles.modalLabel}>To</Text>
              <Text style={styles.modalValue}>
                {mode === "deposit" ? "Earning Balance" : "Available Balance"}
              </Text>
            </View>

            <View style={styles.modalActions}>
              <TouchableOpacity
                style={[styles.modalBtn, styles.modalBtnCancel]}
                onPress={() => setShowConfirm(false)}
              >
                <Text style={styles.modalBtnCancelText}>Cancel</Text>
              </TouchableOpacity>
              <TouchableOpacity
                style={[styles.modalBtn, styles.modalBtnConfirm]}
                onPress={handleConfirm}
              >
                <Text style={styles.modalBtnConfirmText}>Confirm</Text>
              </TouchableOpacity>
            </View>
          </View>
        </View>
      </Modal>
    </SafeAreaView>
  );
}

// ── Styles ────────────────────────────────────────────────────────────────────

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: COLORS.white },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 20,
    paddingVertical: 14,
  },
  backButton: {
    width: 40,
    height: 40,
    borderRadius: 20,
    justifyContent: "center",
    alignItems: "center",
  },
  headerTitle: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  scrollContent: {
    paddingHorizontal: 20,
    paddingBottom: 40,
  },

  // Mode toggle
  toggleRow: {
    flexDirection: "row",
    gap: 12,
    marginBottom: 20,
    marginTop: 4,
  },
  toggleBtn: {
    flex: 1,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 6,
    paddingVertical: 12,
    borderRadius: 12,
    borderWidth: 1.5,
    borderColor: COLORS.primary,
    backgroundColor: COLORS.white,
  },
  toggleBtnActive: {
    backgroundColor: COLORS.primary,
    borderColor: COLORS.primary,
  },
  toggleBtnText: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
  toggleBtnTextActive: {
    color: COLORS.secondary,
  },

  // Balance card
  balanceCard: {
    backgroundColor: "#F0FAF0",
    borderRadius: 14,
    padding: 16,
    marginBottom: 14,
    borderWidth: 1,
    borderColor: "#D0EDD0",
  },
  balanceLabel: {
    fontSize: 12,
    color: "#666",
    fontFamily: "Outfit_400Regular",
    marginBottom: 4,
  },
  balanceAmount: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },

  description: {
    fontSize: 13,
    color: "#777",
    fontFamily: "Outfit_400Regular",
    lineHeight: 20,
    marginBottom: 18,
  },

  // Amount display
  amountDisplay: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    borderWidth: 2,
    borderColor: COLORS.primary,
    borderRadius: 16,
    paddingVertical: 18,
    marginBottom: 6,
    backgroundColor: "#FAFFFE",
  },
  amountDisplayError: {
    borderColor: "#CC0000",
    backgroundColor: "#FFF5F5",
  },
  amountCurrency: {
    fontSize: 28,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
    marginRight: 4,
  },
  amountValue: {
    fontSize: 40,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  amountPlaceholder: {
    color: "#CCCCCC",
  },
  limitError: {
    fontSize: 12,
    color: "#CC0000",
    fontFamily: "Outfit_500Medium",
    textAlign: "center",
    marginBottom: 8,
  },

  // Quick-fill
  quickFillRow: {
    flexDirection: "row",
    gap: 8,
    marginBottom: 16,
  },
  quickFillBtn: {
    flex: 1,
    paddingVertical: 8,
    borderRadius: 8,
    backgroundColor: "#F0F0F0",
    alignItems: "center",
  },
  quickFillText: {
    fontSize: 13,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },

  // Keypad
  keypad: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 10,
    marginBottom: 20,
  },
  keypadKey: {
    width: "30%",
    aspectRatio: 2.2,
    justifyContent: "center",
    alignItems: "center",
    backgroundColor: "#F5F5F5",
    borderRadius: 12,
  },
  keypadKeyText: {
    fontSize: 22,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },

  // Confirm button
  confirmButton: {
    backgroundColor: COLORS.primary,
    borderRadius: 28,
    height: 56,
    justifyContent: "center",
    alignItems: "center",
  },
  confirmButtonDisabled: {
    backgroundColor: "#CCCCCC",
  },
  confirmButtonText: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: COLORS.secondary,
  },

  // Modal
  modalOverlay: {
    flex: 1,
    backgroundColor: "rgba(0,0,0,0.4)",
    justifyContent: "flex-end",
  },
  modalCard: {
    backgroundColor: COLORS.white,
    borderTopLeftRadius: 24,
    borderTopRightRadius: 24,
    padding: 24,
    paddingBottom: 36,
  },
  modalTitle: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
    marginBottom: 20,
    textAlign: "center",
  },
  modalRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    marginBottom: 12,
  },
  modalLabel: {
    fontSize: 14,
    color: "#666",
    fontFamily: "Outfit_400Regular",
  },
  modalValue: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
  modalActions: {
    flexDirection: "row",
    gap: 12,
    marginTop: 24,
  },
  modalBtn: {
    flex: 1,
    height: 52,
    borderRadius: 26,
    justifyContent: "center",
    alignItems: "center",
  },
  modalBtnCancel: {
    borderWidth: 1.5,
    borderColor: COLORS.primary,
  },
  modalBtnConfirm: {
    backgroundColor: COLORS.primary,
  },
  modalBtnCancelText: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
  modalBtnConfirmText: {
    fontSize: 15,
    fontFamily: "Outfit_700Bold",
    color: COLORS.secondary,
  },

  // Success screen
  successContainer: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
    paddingHorizontal: 32,
  },
  successCircle: {
    width: 100,
    height: 100,
    borderRadius: 50,
    borderWidth: 3,
    borderColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 24,
    backgroundColor: "#F0FAF0",
  },
  successTitle: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
    marginBottom: 12,
    textAlign: "center",
  },
  successSubtitle: {
    fontSize: 14,
    color: "#666",
    fontFamily: "Outfit_400Regular",
    textAlign: "center",
    lineHeight: 22,
    marginBottom: 40,
  },
  doneButton: {
    backgroundColor: COLORS.primary,
    borderRadius: 28,
    height: 56,
    width: "100%",
    justifyContent: "center",
    alignItems: "center",
  },
  doneButtonText: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: COLORS.secondary,
  },
});
