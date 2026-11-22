import React, { useEffect, useState } from "react";
import {
  View,
  StyleSheet,
  TextInput,
  Pressable,
  ScrollView,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { Feather } from "@expo/vector-icons";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";

import { ThemedText } from "@/src/components/ThemedText";
import { useTheme } from "@/src/hooks/useTheme";
import { Spacing, BorderRadius } from "@/src/constants/theme";
import {
  fetchAuthenticatedBalance,
  fetchLinkedBankDestination,
  fetchWithdrawalHistory,
  validateWithdrawalAmount,
  WithdrawalError,
  type LinkedBankDestination,
  type WithdrawalHistoryItem,
} from "@/src/services/withdrawalService";

type TabType = "withdraw" | "history";

export default function WithdrawScreen() {
  const insets = useSafeAreaInsets();
  const { theme } = useTheme();
  const router = useRouter();
  const [activeTab, setActiveTab] = useState<TabType>("withdraw");
  const [amount, setAmount] = useState("");
  const [balance, setBalance] = useState<number>(0);
  const [bank, setBank] = useState<LinkedBankDestination | null>(null);
  const [history, setHistory] = useState<WithdrawalHistoryItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const load = React.useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [nextBalance, nextBank, nextHistory] = await Promise.all([
        fetchAuthenticatedBalance(),
        fetchLinkedBankDestination(),
        fetchWithdrawalHistory(),
      ]);
      setBalance(nextBalance);
      setBank(nextBank);
      setHistory(nextHistory);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load withdrawal details.");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const handleTabPress = (tab: TabType) => {
    Haptics.selectionAsync();
    setActiveTab(tab);
  };

  const handleMaxPress = () => {
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
    setAmount(balance.toFixed(2));
    setError("");
  };

  const handleWithdraw = () => {
    try {
      const validated = validateWithdrawalAmount(amount, balance);
      if (!bank) throw new WithdrawalError("Link a bank account before withdrawing.", "NO_DESTINATION");
      const params = new URLSearchParams({
        amount: validated.toFixed(2),
        accountName: bank.accountName,
        accountNumberLast4: bank.accountNumberLast4,
        bankCode: bank.bankCode ?? "",
        bankName: bank.bankName,
        destinationId: bank.id ?? "",
      });
      setError("");
      Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium);
      router.push(`/merchant/transfer-summary?${params.toString()}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Invalid withdrawal.");
      Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error);
    }
  };

  const formatCurrency = (value: number) => {
    return value.toLocaleString("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  };

  const renderWithdrawTab = () => (
    <View style={styles.tabContent}>
      <View
        style={[
          styles.balanceCard,
          {
            backgroundColor: theme.backgroundRoot,
            borderColor: theme.border,
          },
        ]}
      >
        <ThemedText
          style={[styles.balanceLabel, { color: theme.textSecondary }]}
        >
          Available Balance
        </ThemedText>
        <ThemedText style={[styles.balanceAmount, { color: theme.text }]}>
          ${loading ? "—" : formatCurrency(balance)}
        </ThemedText>
      </View>

      <View
        style={[
          styles.amountInputCard,
          {
            backgroundColor: theme.backgroundRoot,
            borderColor: theme.border,
          },
        ]}
      >
        <View style={styles.amountInputRow}>
          <View style={styles.currencyIcon}>
            <ThemedText style={[styles.currencySymbol, { color: theme.text }]}>
              $
            </ThemedText>
          </View>
          <TextInput
            style={[
              styles.amountInput,
              {
                color: theme.text,
              },
            ]}
            placeholder="Amount"
            placeholderTextColor={theme.textSecondary}
            value={amount}
            onChangeText={setAmount}
            keyboardType="decimal-pad"
          />
          <Pressable
            onPress={handleMaxPress}
            style={({ pressed }) => [
              styles.maxButton,
              { opacity: pressed ? 0.7 : 1 },
            ]}
          >
            <ThemedText style={[styles.maxButtonText, { color: theme.text }]}>
              Max
            </ThemedText>
          </Pressable>
        </View>
      </View>

      <View
        style={[
          styles.bankDetailsCard,
          {
            backgroundColor: "#F2F2F2",
            borderWidth: 0,
          },
        ]}
      >
        <ThemedText style={[styles.bankDetailsTitle, { color: theme.text }]}>
          Bank details
        </ThemedText>

        <View style={styles.bankDetailRow}>
          <ThemedText
            style={[styles.bankDetailLabel, { color: theme.textSecondary }]}
          >
            Account name
          </ThemedText>
          <ThemedText style={[styles.bankDetailValue, { color: "#333" }]}>
            {bank?.accountName ?? "No linked account"}
          </ThemedText>
        </View>

        <View style={styles.bankDetailRow}>
          <ThemedText
            style={[styles.bankDetailLabel, { color: theme.textSecondary }]}
          >
            Account number
          </ThemedText>
          <ThemedText style={[styles.bankDetailValue, { color: "#333" }]}>
            {bank ? `•••• ${bank.accountNumberLast4}` : "—"}
          </ThemedText>
        </View>

        <View style={styles.bankDetailRow}>
          <ThemedText
            style={[styles.bankDetailLabel, { color: theme.textSecondary }]}
          >
            Bank name
          </ThemedText>
          <ThemedText style={[styles.bankDetailValue, { color: "#333" }]}>
            {bank?.bankName ?? "—"}
          </ThemedText>
        </View>
      </View>
    </View>
  );

  const renderHistoryTab = () => (
    <View style={styles.tabContent}>
      {history.length > 0 ? (
        history.map((transaction) => (
          <View
            key={transaction.id}
            style={[
              styles.historyCard,
              {
                backgroundColor: "#F9F9F9",
              },
            ]}
          >
            <View style={styles.historyIconCircle}>
              <Feather name="check" size={18} color="#1A4B4A" />
            </View>

            <View style={styles.historyMainInfo}>
              <ThemedText style={styles.historyAccountNumber}>
                {transaction.accountNumber}
              </ThemedText>
              <ThemedText style={styles.historyDateTime}>
                {transaction.date || "—"}
              </ThemedText>
            </View>

            <View style={styles.historyAmountInfo}>
              <ThemedText style={styles.historyUsdAmount}>
                {transaction.currency
                  ? `${transaction.currency} ${formatCurrency(transaction.amount)}`
                  : formatCurrency(transaction.amount)}
              </ThemedText>
            </View>
          </View>
        ))
      ) : (
        <View style={styles.emptyState}>
          <Feather name="inbox" size={48} color={theme.textSecondary} />
          <ThemedText
            style={[styles.emptyStateText, { color: theme.textSecondary }]}
          >
            No transactions yet
          </ThemedText>
        </View>
      )}
    </View>
  );

  return (
    <View style={[styles.container, { backgroundColor: theme.backgroundRoot }]}>
      <View style={[styles.header, { paddingTop: insets.top }]}>
        <Pressable
          onPress={() => router.back()}
          style={styles.headerBackButton}
        >
          <Feather name="arrow-left" size={24} color={theme.text} />
        </Pressable>
        <ThemedText style={styles.headerTitle}>Withdraw to Bank</ThemedText>
        <View style={{ width: 40 }} />
      </View>

      <ScrollView
        style={styles.scrollView}
        contentContainerStyle={[
          styles.scrollContent,
          {
            paddingBottom:
              insets.bottom + Spacing["5xl"] + Spacing.buttonHeight,
          },
        ]}
        showsVerticalScrollIndicator={false}
      >
        <View style={styles.tabContainer}>
          <Pressable
            onPress={() => handleTabPress("withdraw")}
            style={({ pressed }) => [
              styles.tab,
              activeTab === "withdraw"
                ? [styles.tabActive, { borderColor: theme.text }]
                : [styles.tabInactive],
              { opacity: pressed ? 0.7 : 1 },
            ]}
          >
            <ThemedText
              style={[
                styles.tabText,
                activeTab === "withdraw"
                  ? { color: theme.text }
                  : { color: theme.textSecondary },
              ]}
            >
              Withdraw
            </ThemedText>
          </Pressable>

          <Pressable
            onPress={() => handleTabPress("history")}
            style={({ pressed }) => [
              styles.tab,
              activeTab === "history"
                ? [styles.tabActive, { borderColor: theme.text }]
                : [styles.tabInactive],
              { opacity: pressed ? 0.7 : 1 },
            ]}
          >
            <ThemedText
              style={[
                styles.tabText,
                activeTab === "history"
                  ? { color: theme.text }
                  : { color: theme.textSecondary },
              ]}
            >
              History
            </ThemedText>
          </Pressable>
        </View>

        {error ? (
          <View style={styles.errorCard} testID="withdraw-error">
            <ThemedText style={styles.errorText}>{error}</ThemedText>
            <Pressable onPress={load} hitSlop={8} testID="withdraw-retry">
              <ThemedText style={styles.retryText}>Retry</ThemedText>
            </Pressable>
          </View>
        ) : null}

        {activeTab === "withdraw" ? renderWithdrawTab() : renderHistoryTab()}
      </ScrollView>

      {activeTab === "withdraw" ? (
        <View
          style={[
            styles.bottomButtonContainer,
            {
              paddingBottom: insets.bottom + Spacing.lg,
              backgroundColor: theme.backgroundRoot,
            },
          ]}
        >
          <Pressable
            onPress={handleWithdraw}
            style={({ pressed }) => [
              styles.withdrawButton,
              {
                backgroundColor: theme.primary,
                opacity: pressed ? 0.9 : 1,
              },
            ]}
          >
            <ThemedText style={styles.withdrawButtonText}>
              Initiate Withdrawal
            </ThemedText>
          </Pressable>
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  scrollView: {
    flex: 1,
  },
  scrollContent: {
    paddingHorizontal: Spacing.lg,
    paddingTop: Spacing.xl,
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: Spacing.lg,
    paddingVertical: Spacing.lg,
  },
  headerBackButton: {
    width: 40,
    height: 40,
    justifyContent: "center",
  },
  headerTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    flex: 1,
    textAlign: "center",
    marginRight: 40,
  },
  tabContainer: {
    flexDirection: "row",
    gap: Spacing.md,
    marginBottom: Spacing["3xl"],
  },
  tab: {
    paddingHorizontal: Spacing.xl,
    paddingVertical: Spacing.sm,
    borderRadius: BorderRadius.full,
    minWidth: 100,
    alignItems: "center",
  },
  tabActive: {
    borderWidth: 1.5,
  },
  tabInactive: {},
  tabText: {
    fontSize: 15,
    fontFamily: "Outfit_500Medium",
  },
  tabContent: {
    gap: Spacing.xl,
  },
  balanceCard: {
    padding: Spacing["2xl"],
    borderRadius: BorderRadius.xl,
    borderWidth: 1,
    borderColor: "#F0F0F0",
  },
  balanceLabel: {
    fontSize: 15,
    fontFamily: "Outfit_400Regular",
    marginBottom: Spacing.xs,
  },
  balanceAmount: {
    fontSize: 38,
    fontFamily: "Outfit_700Bold",
    letterSpacing: -1,
  },
  errorCard: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    borderRadius: BorderRadius.lg,
    backgroundColor: "#FFF1F0",
    padding: Spacing.lg,
  },
  errorText: {
    flex: 1,
    color: "#B42318",
    fontFamily: "Outfit_500Medium",
  },
  retryText: {
    color: "#1A4B4A",
    fontFamily: "Outfit_700Bold",
  },
  amountInputCard: {
    borderRadius: BorderRadius.xl,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    overflow: "hidden",
  },
  amountInputRow: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: Spacing.lg,
    height: Spacing.inputHeight,
  },
  currencyIcon: {
    marginRight: Spacing.sm,
  },
  currencySymbol: {
    fontSize: 18,
    fontWeight: "500",
  },
  amountInput: {
    flex: 1,
    fontSize: 16,
    height: "100%",
  },
  maxButton: {
    paddingHorizontal: Spacing.md,
    paddingVertical: Spacing.xs,
  },
  maxButtonText: {
    fontSize: 14,
    fontWeight: "600",
  },
  bankDetailsCard: {
    padding: Spacing["2xl"],
    borderRadius: BorderRadius.xl,
    gap: Spacing.lg,
  },
  bankDetailsTitle: {
    fontSize: 18,
    fontFamily: "Outfit_600SemiBold",
    marginBottom: Spacing.xs,
  },
  bankDetailRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
  },
  bankDetailLabel: {
    fontSize: 15,
    fontFamily: "Outfit_400Regular",
  },
  bankDetailValue: {
    fontSize: 15,
    fontFamily: "Outfit_500Medium",
  },
  historyCard: {
    flexDirection: "row",
    alignItems: "center",
    padding: Spacing.xl,
    borderRadius: BorderRadius.xl,
    gap: Spacing.md,
  },
  historyIconCircle: {
    width: 40,
    height: 40,
    borderRadius: 20,
    backgroundColor: "#D1F7D6",
    alignItems: "center",
    justifyContent: "center",
  },
  historyMainInfo: {
    flex: 1,
    gap: 4,
  },
  historyAccountNumber: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: "#333",
  },
  historyDateTime: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  historyAmountInfo: {
    alignItems: "flex-end",
    gap: 4,
  },
  historyUsdAmount: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: "#333",
  },
  emptyState: {
    alignItems: "center",
    justifyContent: "center",
    paddingVertical: Spacing["5xl"],
    gap: Spacing.md,
  },
  emptyStateText: {
    fontSize: 16,
  },
  bottomButtonContainer: {
    position: "absolute",
    bottom: 0,
    left: 0,
    right: 0,
    paddingHorizontal: Spacing.lg,
    paddingTop: Spacing.lg,
  },
  withdrawButton: {
    height: Spacing.buttonHeight,
    borderRadius: BorderRadius.full,
    alignItems: "center",
    justifyContent: "center",
  },
  withdrawButtonText: {
    color: "#FFFFFF",
    fontSize: 16,
    fontWeight: "600",
  },
});
