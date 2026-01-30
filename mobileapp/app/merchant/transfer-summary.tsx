import React from "react";
import {
  View,
  StyleSheet,
  Pressable,
  ScrollView,
  Animated,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { Feather } from "@expo/vector-icons";
import * as Haptics from "expo-haptics";
import { useRouter } from "expo-router";

import { ThemedText } from "@/src/components/ThemedText";
import { useTheme } from "@/src/hooks/useTheme";
import { Spacing, BorderRadius } from "@/src/constants/theme";



export default function TransferSummaryScreen() {
  const insets = useSafeAreaInsets();

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
      router.push("/merchant/success");
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
      <View style={[styles.header, { paddingTop: insets.top }]}>
        <Pressable onPress={() => router.back()} style={styles.headerBackButton}>
          <Feather name="arrow-left" size={24} color={theme.text} />
        </Pressable>
        <ThemedText style={styles.headerTitle}>Summary & confirmation</ThemedText>
        <View style={{ width: 40 }} />
      </View>

      <ScrollView
        style={styles.scrollView}
        contentContainerStyle={[
          styles.scrollContent,
          {
            paddingBottom:
              insets.bottom + Spacing["5xl"] + Spacing.buttonHeight * 2,
          },
        ]}
        showsVerticalScrollIndicator={false}
      >
        <View style={styles.summaryCard}>
          <View style={styles.iconCircle}>
            <View style={styles.iconInner}>
              <ThemedText style={styles.dollarSign}>$</ThemedText>
            </View>
          </View>

          <ThemedText style={styles.amountText}>
            ${formatCurrency(15046.12)}
          </ThemedText>

          <View style={styles.recipientRow}>
            <View style={styles.bankIconBg}>
              <Feather name="home" size={20} color="#333" />
            </View>
            <View style={styles.recipientDetails}>
              <ThemedText style={styles.recipientLabel}>Recipient bank account</ThemedText>
              <ThemedText style={styles.recipientNumber}>91235704180</ThemedText>
            </View>
          </View>

          <View style={styles.divider} />

          <View style={styles.dateRow}>
            <ThemedText style={styles.dateLabel}>Date</ThemedText>
            <ThemedText style={styles.dateValue}>Nov 12 2025, 8.12 AM</ThemedText>
          </View>
        </View>
      </ScrollView>

      <View
        style={[
          styles.bottomButtonContainer,
          {
            paddingBottom: insets.bottom + Spacing.lg,
          },
        ]}
      >
        <Animated.View style={{ transform: [{ scale: scaleAnim }], gap: Spacing.md }}>
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
              Confirm Withdrawal
            </ThemedText>
          </Pressable>

          <Pressable
            onPress={() => router.back()}
            style={({ pressed }) => [
              styles.cancelButton,
              {
                backgroundColor: "#8CF79C",
                opacity: pressed ? 0.9 : 1,
              },
            ]}
          >
            <ThemedText style={styles.cancelButtonText}>
              Cancel
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
  summaryCard: {
    padding: Spacing["2xl"],
    borderRadius: BorderRadius.xl,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    backgroundColor: "#FFFFFF",
    alignItems: "center",
  },
  iconCircle: {
    width: 80,
    height: 80,
    borderRadius: 40,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    alignItems: "center",
    justifyContent: "center",
    marginBottom: Spacing.xl,
  },
  iconInner: {
    width: 60,
    height: 60,
    borderRadius: 30,
    borderWidth: 2,
    borderColor: "#1A4B4A",
    alignItems: "center",
    justifyContent: "center",
  },
  dollarSign: {
    fontSize: 30,
    fontFamily: "Outfit_700Bold",
    color: "#1A4B4A",
  },
  amountText: {
    fontSize: 32,
    fontFamily: "Outfit_700Bold",
    marginBottom: Spacing["2xl"],
  },
  recipientRow: {
    flexDirection: "row",
    alignItems: "center",
    width: "100%",
    gap: Spacing.md,
    marginBottom: Spacing.xl,
  },
  bankIconBg: {
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: "#F5F5F5",
    alignItems: "center",
    justifyContent: "center",
  },
  recipientDetails: {
    flex: 1,
  },
  recipientLabel: {
    fontSize: 13,
    color: "#666",
    fontFamily: "Outfit_400Regular",
  },
  recipientNumber: {
    fontSize: 15,
    fontFamily: "Outfit_500Medium",
    color: "#333",
  },
  divider: {
    height: 1,
    backgroundColor: "#F0F0F0",
    width: "100%",
    marginBottom: Spacing.xl,
  },
  dateRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    width: "100%",
    alignItems: "center",
  },
  dateLabel: {
    fontSize: 14,
    color: "#999",
    fontFamily: "Outfit_400Regular",
  },
  dateValue: {
    fontSize: 14,
    color: "#333",
    fontFamily: "Outfit_500Medium",
  },
  bottomButtonContainer: {
    position: "absolute",
    bottom: 0,
    left: 0,
    right: 0,
    paddingHorizontal: Spacing.lg,
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
    fontFamily: "Outfit_600SemiBold",
  },
  cancelButton: {
    height: Spacing.buttonHeight,
    borderRadius: BorderRadius.full,
    alignItems: "center",
    justifyContent: "center",
  },
  cancelButtonText: {
    color: "#1A4B4A",
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
  },
});
