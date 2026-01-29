import React, { useEffect, useRef } from "react";
import { View, StyleSheet, Pressable, Animated, Easing } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { Feather } from "@expo/vector-icons";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";

import { ThemedText } from "@/src/components/ThemedText";
import { useTheme } from "@/src/hooks/useTheme";
import { Spacing, BorderRadius } from "@/src/constants/theme";

const MOCK_TRANSACTION = {
  type: "Withdrawal",
  amount: 15000.0,
  recipient: "Opay Bank",
  date: "Jan 29, 2026",
  time: "9:41 AM",
  reference: "ZAP-2026-0129-001",
};

export default function SuccessScreen() {
  const insets = useSafeAreaInsets();
  const { theme } = useTheme();
  const router = useRouter();

  const scaleAnim = useRef(new Animated.Value(0)).current;
  const checkAnim = useRef(new Animated.Value(0)).current;
  const fadeAnim = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);

    // Success animation sequence
    Animated.sequence([
      Animated.parallel([
        Animated.spring(scaleAnim, {
          toValue: 1,
          tension: 50,
          friction: 7,
          useNativeDriver: true,
        }),
        Animated.timing(fadeAnim, {
          toValue: 1,
          duration: 300,
          useNativeDriver: true,
        }),
      ]),
      Animated.timing(checkAnim, {
        toValue: 1,
        duration: 400,
        easing: Easing.bezier(0.25, 0.1, 0.25, 1),
        useNativeDriver: true,
      }),
    ]).start();
  }, [scaleAnim, fadeAnim, checkAnim]);

  const formatCurrency = (value: number) => {
    return value.toLocaleString("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  };

  const handleDone = () => {
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
    router.push("/");
  };

  const handleViewReceipt = () => {
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
    // Navigate to receipt screen
  };

  const checkScale = checkAnim.interpolate({
    inputRange: [0, 0.5, 1],
    outputRange: [0, 1.2, 1],
  });

  return (
    <View style={[styles.container, { backgroundColor: theme.backgroundRoot }]}>
      <View
        style={[
          styles.content,
          {
            paddingTop: insets.top + Spacing["5xl"],
            paddingBottom: insets.bottom + Spacing["2xl"],
          },
        ]}
      >
        <Animated.View
          style={[
            styles.successIconContainer,
            {
              backgroundColor: theme.primary + "15",
              transform: [{ scale: scaleAnim }],
              opacity: fadeAnim,
            },
          ]}
        >
          <Animated.View
            style={{
              transform: [{ scale: checkScale }],
            }}
          >
            <Feather name="check" size={48} color={theme.primary} />
          </Animated.View>
        </Animated.View>

        <Animated.View style={[styles.textContainer, { opacity: fadeAnim }]}>
          <ThemedText style={[styles.title, { color: theme.text }]}>
            Transaction Successful!
          </ThemedText>
          <ThemedText style={[styles.subtitle, { color: theme.textSecondary }]}>
            Your {MOCK_TRANSACTION.type.toLowerCase()} has been processed
          </ThemedText>
        </Animated.View>

        <Animated.View
          style={[
            styles.detailsCard,
            {
              backgroundColor: theme.backgroundDefault,
              borderColor: theme.border,
              opacity: fadeAnim,
            },
          ]}
        >
          <View style={styles.amountSection}>
            <ThemedText
              style={[styles.amountLabel, { color: theme.textSecondary }]}
            >
              Amount
            </ThemedText>
            <ThemedText style={[styles.amountValue, { color: theme.text }]}>
              ${formatCurrency(MOCK_TRANSACTION.amount)}
            </ThemedText>
          </View>

          <View style={[styles.divider, { backgroundColor: theme.border }]} />

          <View style={styles.infoRow}>
            <ThemedText
              style={[styles.infoLabel, { color: theme.textSecondary }]}
            >
              To
            </ThemedText>
            <ThemedText style={[styles.infoValue, { color: theme.text }]}>
              {MOCK_TRANSACTION.recipient}
            </ThemedText>
          </View>

          <View style={styles.infoRow}>
            <ThemedText
              style={[styles.infoLabel, { color: theme.textSecondary }]}
            >
              Date
            </ThemedText>
            <ThemedText style={[styles.infoValue, { color: theme.text }]}>
              {MOCK_TRANSACTION.date}
            </ThemedText>
          </View>

          <View style={styles.infoRow}>
            <ThemedText
              style={[styles.infoLabel, { color: theme.textSecondary }]}
            >
              Time
            </ThemedText>
            <ThemedText style={[styles.infoValue, { color: theme.text }]}>
              {MOCK_TRANSACTION.time}
            </ThemedText>
          </View>

          <View style={[styles.divider, { backgroundColor: theme.border }]} />

          <View style={styles.referenceContainer}>
            <ThemedText
              style={[styles.referenceLabel, { color: theme.textSecondary }]}
            >
              Reference Number
            </ThemedText>
            <ThemedText style={[styles.referenceValue, { color: theme.text }]}>
              {MOCK_TRANSACTION.reference}
            </ThemedText>
          </View>
        </Animated.View>

        <Animated.View style={[styles.buttonContainer, { opacity: fadeAnim }]}>
          <Pressable
            onPress={handleViewReceipt}
            style={({ pressed }) => [
              styles.secondaryButton,
              {
                borderColor: theme.border,
                opacity: pressed ? 0.7 : 1,
              },
            ]}
          >
            <Feather name="download" size={20} color={theme.text} />
            <ThemedText
              style={[styles.secondaryButtonText, { color: theme.text }]}
            >
              Download Receipt
            </ThemedText>
          </Pressable>

          <Pressable
            onPress={handleDone}
            style={({ pressed }) => [
              styles.primaryButton,
              {
                backgroundColor: theme.primary,
                opacity: pressed ? 0.9 : 1,
              },
            ]}
          >
            <ThemedText style={styles.primaryButtonText}>Done</ThemedText>
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
  content: {
    flex: 1,
    paddingHorizontal: Spacing.lg,
    alignItems: "center",
    justifyContent: "center",
  },
  successIconContainer: {
    width: 120,
    height: 120,
    borderRadius: 60,
    alignItems: "center",
    justifyContent: "center",
    marginBottom: Spacing["2xl"],
  },
  textContainer: {
    alignItems: "center",
    marginBottom: Spacing["4xl"],
  },
  title: {
    fontSize: 28,
    fontWeight: "700",
    marginBottom: Spacing.sm,
    textAlign: "center",
  },
  subtitle: {
    fontSize: 16,
    textAlign: "center",
  },
  detailsCard: {
    width: "100%",
    padding: Spacing.xl,
    borderRadius: BorderRadius.md,
    borderWidth: 1,
    marginBottom: Spacing["4xl"],
  },
  amountSection: {
    alignItems: "center",
    paddingVertical: Spacing.md,
  },
  amountLabel: {
    fontSize: 14,
    marginBottom: Spacing.xs,
  },
  amountValue: {
    fontSize: 42,
    fontWeight: "700",
    letterSpacing: -1,
  },
  divider: {
    height: 1,
    marginVertical: Spacing.lg,
  },
  infoRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    paddingVertical: Spacing.sm,
  },
  infoLabel: {
    fontSize: 14,
  },
  infoValue: {
    fontSize: 14,
    fontWeight: "500",
  },
  referenceContainer: {
    alignItems: "center",
    paddingTop: Spacing.sm,
  },
  referenceLabel: {
    fontSize: 12,
    marginBottom: Spacing.xs,
  },
  referenceValue: {
    fontSize: 12,
    fontWeight: "600",
    letterSpacing: 0.5,
  },
  buttonContainer: {
    width: "100%",
    gap: Spacing.md,
  },
  primaryButton: {
    height: Spacing.buttonHeight,
    borderRadius: BorderRadius.full,
    alignItems: "center",
    justifyContent: "center",
  },
  primaryButtonText: {
    color: "#FFFFFF",
    fontSize: 16,
    fontWeight: "600",
  },
  secondaryButton: {
    height: Spacing.buttonHeight,
    borderRadius: BorderRadius.full,
    alignItems: "center",
    justifyContent: "center",
    flexDirection: "row",
    gap: Spacing.sm,
    borderWidth: 1,
  },
  secondaryButtonText: {
    fontSize: 16,
    fontWeight: "500",
  },
});
