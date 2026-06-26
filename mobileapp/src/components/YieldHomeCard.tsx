import React from "react";
import { View, Text, StyleSheet, Switch, TouchableOpacity } from "react-native";
import { COLORS } from "../constants/colors";

export interface YieldHomeCardProps {
  /** Formatted earning / yield balance, e.g. "₦3,280.45". */
  earningBalance: string;
  /** Whether auto-yield deposits are currently enabled. */
  autoYieldEnabled: boolean;
  /** Called with the next value when the auto-yield switch is toggled. */
  onToggleAutoYield: (value: boolean) => void;
  /** Optional press handler for opening the yield breakdown. */
  onPress?: () => void;
}

/**
 * Home Screen card that surfaces the user's earning balance, an active
 * "money is working" status and an auto-yield deposit toggle.
 */
export default function YieldHomeCard({
  earningBalance,
  autoYieldEnabled,
  onToggleAutoYield,
  onPress,
}: YieldHomeCardProps) {
  return (
    <TouchableOpacity
      style={styles.card}
      activeOpacity={0.9}
      onPress={onPress}
      accessibilityRole="button"
    >
      <View style={styles.main}>
        <Text style={styles.label}>Earning Balance</Text>
        <Text style={styles.amount}>{earningBalance}</Text>
        <View style={styles.statusRow}>
          <View style={styles.statusDot} />
          <Text style={styles.statusText}>Your money is working</Text>
        </View>
      </View>

      <View style={styles.toggleWrap}>
        <Text style={styles.toggleLabel}>Auto-yield</Text>
        <Switch
          testID="auto-yield-switch"
          value={autoYieldEnabled}
          onValueChange={onToggleAutoYield}
          trackColor={{ false: "#E2E8F0", true: "#34D399" }}
          thumbColor={COLORS.white}
        />
      </View>
    </TouchableOpacity>
  );
}

const styles = StyleSheet.create({
  card: {
    backgroundColor: "#0F3D2E",
    borderRadius: 22,
    paddingVertical: 18,
    paddingHorizontal: 18,
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
  },
  main: {
    flex: 1,
  },
  label: {
    fontSize: 13,
    fontFamily: "Outfit_500Medium",
    color: "#9FD9B5",
    marginBottom: 6,
  },
  amount: {
    fontSize: 24,
    fontFamily: "Outfit_700Bold",
    color: COLORS.white,
    marginBottom: 8,
  },
  statusRow: {
    flexDirection: "row",
    alignItems: "center",
  },
  statusDot: {
    width: 7,
    height: 7,
    borderRadius: 4,
    backgroundColor: "#34D399",
    marginRight: 7,
  },
  statusText: {
    fontSize: 12,
    fontFamily: "Outfit_500Medium",
    color: "#BFE9CF",
  },
  toggleWrap: {
    alignItems: "center",
    marginLeft: 12,
  },
  toggleLabel: {
    fontSize: 11,
    fontFamily: "Outfit_500Medium",
    color: "#9FD9B5",
    marginBottom: 6,
  },
});
