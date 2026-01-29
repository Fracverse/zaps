import React from "react";
import {
  View,
  StyleSheet,
  Pressable,
  ScrollView,
  Animated,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { useHeaderHeight } from "@react-navigation/elements";
import { Feather } from "@expo/vector-icons";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";

import { ThemedText } from "@/src/components/ThemedText";
import { useTheme } from "@/src/hooks/useTheme";
import { Spacing, BorderRadius } from "@/src/constants/theme";

const MOCK_TRANSFER = {
  recipient: "John Doe",
  recipientId: "@johndoe",
  amount: 250.0,
  fee: 2.5,
  total: 252.5,
  note: "Payment for services",
};

export default function TransferSummaryScreen() {
  const insets = useSafeAreaInsets();
  const headerHeight = useHeaderHeight();
  const { theme } = useTheme();
  const router = useRouter();
  const scaleAnim = React.useRef(new Animated.Value(1)).current;

  const handleConfirm = () => {
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium);
    
    // Button press animation
    Animated.sequence([
      Animated.timing(scaleAnim, {
        toValue: 0.95,
        duration: 100,
        useNativeDriver: true,
      }),
      Animated.timing(scaleAnim, {
        toValue: 1,
        duration: 100,
        useNativeDriver: true,
      }),
    ]).start(() => {
      router.push("/merchant/transfer-confirmation");
    });
  };

  const formatCurrency = (value: number) => {
    return value.toLocaleString("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  };

  return (
    <View style={[styles.container, { backgroundColor: theme.backgroundRoot }]}>
      <ScrollView
        style={styles.scrollView}
        contentContainerStyle={[
          styles.scrollContent,
          {
            paddingTop: headerHeight + Spacing.lg,
            paddingBottom: insets.bottom + Spacing["5xl"] + Spacing.buttonHeight,
          },
        ]}
        showsVerticalScrollIndicator={false}
      >
        <ThemedText style={[styles.title, { color: theme.text }]}>
          Transfer Summary
        </ThemedText>

        <View style={styles.content}>
          {/* Recipient Card */}
          <View
            style={[
              styles.card,
              {
                backgroundColor: theme.backgroundDefault,
                borderColor: theme.border,
              },
            ]}
          >
            <View style={styles.recipientHeader}>
              <View
                style={[
                  styles.avatarCircle,
                  { backgroundColor: theme.primary + "20" },
                ]}
              >
                <Feather name="user" size={24} color={theme.primary} />
              </View>
              <View style={styles.recipientInfo}>
                <ThemedText style={[styles.recipientName, { color: theme.text }]}>
                  {MOCK_TRANSFER.recipient}
                </ThemedText>
                <ThemedText
                  style={[styles.recipientId, { color: theme.textSecondary }]}
                >
                  {MOCK_TRANSFER.recipientId}
                </ThemedText>
              </View>
            </View>
          </View>

          {/* Amount Card */}
          <View
            style={[
              styles.card,
              styles.amountCard,
              {
                backgroundColor: theme.backgroundRoot,
                borderColor: theme.border,
              },
            ]}
          >
            <ThemedText
              style={[styles.amountLabel, { color: theme.textSecondary }]}
            >
              Amount
            </ThemedText>
            <ThemedText style={[styles.amountValue, { color: theme.text }]}>
              ${formatCurrency(MOCK_TRANSFER.amount)}
            </ThemedText>
          </View>

          {/* Details Card */}
          <View
            style={[
              styles.card,
              {
                backgroundColor: theme.backgroundDefault,
                borderColor: theme.border,
              },
            ]}
          >
            <ThemedText style={[styles.detailsTitle, { color: theme.text }]}>
              Transaction Details
            </ThemedText>

            <View style={styles.detailRow}>
              <ThemedText
                style={[styles.detailLabel, { color: theme.textSecondary }]}
              >
                Transfer Amount
              </ThemedText>
              <ThemedText style={[styles.detailValue, { color: theme.text }]}>
                ${formatCurrency(MOCK_TRANSFER.amount)}
              </ThemedText>
            </View>

            <View style={styles.detailRow}>
              <ThemedText
                style={[styles.detailLabel, { color: theme.textSecondary }]}
              >
                Transaction Fee
              </ThemedText>
              <ThemedText style={[styles.detailValue, { color: theme.text }]}>
                ${formatCurrency(MOCK_TRANSFER.fee)}
              </ThemedText>
            </View>

            <View style={[styles.divider, { backgroundColor: theme.border }]} />

            <View style={styles.detailRow}>
              <ThemedText style={[styles.totalLabel, { color: theme.text }]}>
                Total Amount
              </ThemedText>
              <ThemedText style={[styles.totalValue, { color: theme.text }]}>
                ${formatCurrency(MOCK_TRANSFER.total)}
              </ThemedText>
            </View>

            {MOCK_TRANSFER.note && (
              <>
                <View style={[styles.divider, { backgroundColor: theme.border }]} />
                <View style={styles.noteContainer}>
                  <ThemedText
                    style={[styles.noteLabel, { color: theme.textSecondary }]}
                  >
                    Note
                  </ThemedText>
                  <ThemedText style={[styles.noteText, { color: theme.text }]}>
                    {MOCK_TRANSFER.note}
                  </ThemedText>
                </View>
              </>
            )}
          </View>
        </View>
      </ScrollView>

      <View
        style={[
          styles.bottomButtonContainer,
          {
            paddingBottom: insets.bottom + Spacing.lg,
            backgroundColor: theme.backgroundRoot,
          },
        ]}
      >
        <Animated.View style={{ transform: [{ scale: scaleAnim }] }}>
          <Pressable
            onPress={handleConfirm}
            style={({ pressed }) => [
              styles.confirmButton,
              {
                backgroundColor: theme.primary,
                opacity: pressed ? 0.9 : 1,
              },
            ]}
          >
            <ThemedText style={styles.confirmButtonText}>
              Confirm Transfer
            </ThemedText>
          </Pressable>
        </Animated.View>
      </View>
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
  title: {
    fontSize: 28,
    fontWeight: "700",
    marginBottom: Spacing["2xl"],
  },
  content: {
    gap: Spacing.lg,
  },
  card: {
    padding: Spacing.xl,
    borderRadius: BorderRadius.md,
    borderWidth: 1,
  },
  recipientHeader: {
    flexDirection: "row",
    alignItems: "center",
    gap: Spacing.md,
  },
  avatarCircle: {
    width: 56,
    height: 56,
    borderRadius: 28,
    alignItems: "center",
    justifyContent: "center",
  },
  recipientInfo: {
    flex: 1,
  },
  recipientName: {
    fontSize: 18,
    fontWeight: "600",
    marginBottom: Spacing.xs,
  },
  recipientId: {
    fontSize: 14,
  },
  amountCard: {
    alignItems: "center",
  },
  amountLabel: {
    fontSize: 14,
    marginBottom: Spacing.sm,
  },
  amountValue: {
    fontSize: 48,
    fontWeight: "700",
    letterSpacing: -2,
  },
  detailsTitle: {
    fontSize: 16,
    fontWeight: "600",
    marginBottom: Spacing.md,
  },
  detailRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    paddingVertical: Spacing.xs,
  },
  detailLabel: {
    fontSize: 14,
  },
  detailValue: {
    fontSize: 14,
    fontWeight: "500",
  },
  divider: {
    height: 1,
    marginVertical: Spacing.sm,
  },
  totalLabel: {
    fontSize: 16,
    fontWeight: "600",
  },
  totalValue: {
    fontSize: 18,
    fontWeight: "700",
  },
  noteContainer: {
    gap: Spacing.xs,
  },
  noteLabel: {
    fontSize: 12,
    textTransform: "uppercase",
  },
  noteText: {
    fontSize: 14,
    lineHeight: 20,
  },
  bottomButtonContainer: {
    position: "absolute",
    bottom: 0,
    left: 0,
    right: 0,
    paddingHorizontal: Spacing.lg,
    paddingTop: Spacing.lg,
  },
  confirmButton: {
    height: Spacing.buttonHeight,
    borderRadius: BorderRadius.full,
    alignItems: "center",
    justifyContent: "center",
  },
  confirmButtonText: {
    color: "#FFFFFF",
    fontSize: 16,
    fontWeight: "600",
  },
});
