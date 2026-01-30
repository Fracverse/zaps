import React, { useState } from "react";
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
import { Spacing, BorderRadius, Colors } from "@/src/constants/theme";

const MOCK_BALANCE = 15046.12;
const MOCK_BANK = {
  accountName: "Ebube One",
  accountNumber: "91235704180",
  bankName: "Opay",
};

type TabType = "withdraw" | "history";

interface Transaction {
  id: string;
  amount: number;
  bankName: string;
  date: string;
  status: "completed" | "pending" | "failed";
}

const MOCK_TRANSACTIONS: Transaction[] = [
  {
    id: "1",
    amount: 500.0,
    bankName: "Opay",
    date: "Jan 28, 2026",
    status: "completed",
  },
  {
    id: "2",
    amount: 1200.0,
    bankName: "Opay",
    date: "Jan 25, 2026",
    status: "completed",
  },
  {
    id: "3",
    amount: 350.0,
    bankName: "Opay",
    date: "Jan 20, 2026",
    status: "pending",
  },
];

export default function WithdrawScreen() {
  const insets = useSafeAreaInsets();
  const { theme } = useTheme();
  const router = useRouter();
  const [activeTab, setActiveTab] = useState<TabType>("withdraw");
  const [amount, setAmount] = useState("");

  const handleTabPress = (tab: TabType) => {
    Haptics.selectionAsync();
    setActiveTab(tab);
  };

  const handleMaxPress = () => {
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
    setAmount(MOCK_BALANCE.toFixed(2));
  };

  const handleWithdraw = () => {
    Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);
    // Navigate to success screen
    router.push("/merchant/success");
  };

  const formatCurrency = (value: number) => {
    return value.toLocaleString("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  };

  const getStatusColor = (status: Transaction["status"]) => {
    switch (status) {
      case "completed":
        return Colors.light.success;
      case "pending":
        return Colors.light.warning;
      case "failed":
        return Colors.light.error;
    }
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
          ${formatCurrency(MOCK_BALANCE)}
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
            <ThemedText
              style={[styles.maxButtonText, { color: theme.primary }]}
            >
              Max
            </ThemedText>
          </Pressable>
        </View>
      </View>

      <View
        style={[
          styles.bankDetailsCard,
          {
            backgroundColor: theme.backgroundDefault,
            borderColor: theme.border,
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
          <ThemedText style={[styles.bankDetailValue, { color: theme.text }]}>
            {MOCK_BANK.accountName}
          </ThemedText>
        </View>

        <View style={styles.bankDetailRow}>
          <ThemedText
            style={[styles.bankDetailLabel, { color: theme.textSecondary }]}
          >
            Account number
          </ThemedText>
          <ThemedText style={[styles.bankDetailValue, { color: theme.text }]}>
            {MOCK_BANK.accountNumber}
          </ThemedText>
        </View>

        <View style={styles.bankDetailRow}>
          <ThemedText
            style={[styles.bankDetailLabel, { color: theme.textSecondary }]}
          >
            Bank name
          </ThemedText>
          <ThemedText style={[styles.bankDetailValue, { color: theme.text }]}>
            {MOCK_BANK.bankName}
          </ThemedText>
        </View>
      </View>
    </View>
  );

  const renderHistoryTab = () => (
    <View style={styles.tabContent}>
      {MOCK_TRANSACTIONS.length > 0 ? (
        MOCK_TRANSACTIONS.map((transaction) => (
          <View
            key={transaction.id}
            style={[
              styles.transactionCard,
              {
                backgroundColor: theme.backgroundRoot,
                borderColor: theme.border,
              },
            ]}
          >
            <View style={styles.transactionLeft}>
              <ThemedText
                style={[styles.transactionAmount, { color: theme.text }]}
              >
                -${formatCurrency(transaction.amount)}
              </ThemedText>
              <ThemedText
                style={[styles.transactionBank, { color: theme.textSecondary }]}
              >
                {transaction.bankName}
              </ThemedText>
            </View>
            <View style={styles.transactionRight}>
              <ThemedText
                style={[styles.transactionDate, { color: theme.textSecondary }]}
              >
                {transaction.date}
              </ThemedText>
              <View
                style={[
                  styles.statusBadge,
                  {
                    backgroundColor: `${getStatusColor(transaction.status)}15`,
                  },
                ]}
              >
                <ThemedText
                  style={[
                    styles.statusText,
                    { color: getStatusColor(transaction.status) },
                  ]}
                >
                  {transaction.status.charAt(0).toUpperCase() +
                    transaction.status.slice(1)}
                </ThemedText>
              </View>
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
      <ScrollView
        style={styles.scrollView}
        contentContainerStyle={[
          styles.scrollContent,
          {
            paddingTop: insets.top + Spacing.md,
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
                : [
                    styles.tabInactive,
                    { backgroundColor: theme.backgroundDefault },
                  ],
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
                : [
                    styles.tabInactive,
                    { backgroundColor: theme.backgroundDefault },
                  ],
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
  },
  tabContainer: {
    flexDirection: "row",
    gap: Spacing.md,
    marginBottom: Spacing["2xl"],
  },
  tab: {
    paddingHorizontal: Spacing.lg,
    paddingVertical: Spacing.sm,
    borderRadius: BorderRadius.full,
  },
  tabActive: {
    borderWidth: 1,
  },
  tabInactive: {},
  tabText: {
    fontSize: 14,
    fontWeight: "500",
  },
  tabContent: {
    gap: Spacing.lg,
  },
  balanceCard: {
    padding: Spacing.xl,
    borderRadius: BorderRadius.md,
    borderWidth: 1,
  },
  balanceLabel: {
    fontSize: 14,
    marginBottom: Spacing.xs,
  },
  balanceAmount: {
    fontSize: 36,
    fontWeight: "700",
    letterSpacing: -1,
  },
  amountInputCard: {
    borderRadius: BorderRadius.md,
    borderWidth: 1,
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
    padding: Spacing.xl,
    borderRadius: BorderRadius.md,
    borderWidth: 1,
    gap: Spacing.md,
  },
  bankDetailsTitle: {
    fontSize: 16,
    fontWeight: "600",
    marginBottom: Spacing.xs,
  },
  bankDetailRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
  },
  bankDetailLabel: {
    fontSize: 14,
  },
  bankDetailValue: {
    fontSize: 14,
    fontWeight: "500",
  },
  transactionCard: {
    flexDirection: "row",
    justifyContent: "space-between",
    padding: Spacing.lg,
    borderRadius: BorderRadius.md,
    borderWidth: 1,
  },
  transactionLeft: {
    gap: Spacing.xs,
  },
  transactionAmount: {
    fontSize: 16,
    fontWeight: "600",
  },
  transactionBank: {
    fontSize: 14,
  },
  transactionRight: {
    alignItems: "flex-end",
    gap: Spacing.xs,
  },
  transactionDate: {
    fontSize: 12,
  },
  statusBadge: {
    paddingHorizontal: Spacing.sm,
    paddingVertical: Spacing.xs,
    borderRadius: 6,
  },
  statusText: {
    fontSize: 12,
    fontWeight: "500",
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
