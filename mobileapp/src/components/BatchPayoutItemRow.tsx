import React from "react";
import { View, Text, StyleSheet } from "react-native";
import { COLORS } from "../constants/colors";
import type { BatchPayoutItem } from "../types/batchPayout";

const STATUS_STYLES: Record<
  BatchPayoutItem["status"],
  { bg: string; text: string; label: string }
> = {
  pending: { bg: "#FEF3C7", text: "#92400E", label: "Pending" },
  completed: { bg: "#D1FAE5", text: "#065F46", label: "Completed" },
  failed: { bg: "#FEE2E2", text: "#991B1B", label: "Failed" },
};

export default function BatchPayoutItemRow({ item }: { item: BatchPayoutItem }) {
  const status = STATUS_STYLES[item.status];

  return (
    <View style={styles.row}>
      <View style={styles.info}>
        <Text style={styles.name}>{item.recipientName}</Text>
        <Text style={styles.address}>{item.recipientAddress}</Text>
      </View>
      <View style={styles.right}>
        <Text style={styles.amount}>
          {item.amount} {item.currency}
        </Text>
        <View style={[styles.badge, { backgroundColor: status.bg }]}>
          <Text style={[styles.badgeText, { color: status.text }]}>
            {status.label}
          </Text>
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    paddingVertical: 12,
    borderBottomWidth: 1,
    borderBottomColor: "#E5E7EB",
  },
  info: {
    flex: 1,
    marginRight: 12,
  },
  name: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    marginBottom: 2,
  },
  address: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#6B7280",
  },
  right: {
    alignItems: "flex-end",
  },
  amount: {
    fontSize: 15,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 4,
  },
  badge: {
    paddingHorizontal: 8,
    paddingVertical: 2,
    borderRadius: 10,
  },
  badgeText: {
    fontSize: 11,
    fontFamily: "Outfit_500Medium",
  },
});
