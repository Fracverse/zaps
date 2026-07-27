import React from "react";
import { View, Text, StyleSheet } from "react-native";
import { COLORS } from "../constants/colors";
import type {
  BatchPayoutSummary as BatchPayoutSummaryType,
  BatchPayoutItem,
} from "../types/batchPayout";
import BatchPayoutItemRow from "./BatchPayoutItemRow";

interface Props {
  summary: BatchPayoutSummaryType;
  items: BatchPayoutItem[];
}

export default function BatchPayoutSummary({ summary, items }: Props) {
  const pendingCount = summary.itemCount - summary.completedCount - summary.failedCount;

  return (
    <View style={styles.card}>
      <Text style={styles.title}>Batch Payout</Text>

      <Text style={styles.totalAmount}>
        {summary.totalAmount} {summary.currency}
      </Text>

      <View style={styles.statsRow}>
        <Text style={styles.stat}>
          {summary.itemCount} item{summary.itemCount !== 1 ? "s" : ""}
        </Text>
        <Text style={[styles.stat, styles.completedStat]}>
          {summary.completedCount} completed
        </Text>
        {summary.failedCount > 0 && (
          <Text style={[styles.stat, styles.failedStat]}>
            {summary.failedCount} failed
          </Text>
        )}
        {pendingCount > 0 && (
          <Text style={[styles.stat, styles.pendingStat]}>
            {pendingCount} pending
          </Text>
        )}
      </View>

      <View style={styles.list}>
        {items.length === 0 ? (
          <Text style={styles.emptyText}>No payout items</Text>
        ) : (
          items.map((item) => (
            <BatchPayoutItemRow key={item.id} item={item} />
          ))
        )}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    backgroundColor: COLORS.white,
    borderRadius: 22,
    padding: 18,
  },
  title: {
    fontSize: 13,
    fontFamily: "Outfit_500Medium",
    color: "#6B7280",
    marginBottom: 6,
  },
  totalAmount: {
    fontSize: 24,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
    marginBottom: 8,
  },
  statsRow: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 10,
    marginBottom: 16,
  },
  stat: {
    fontSize: 12,
    fontFamily: "Outfit_500Medium",
    color: "#6B7280",
  },
  completedStat: {
    color: "#065F46",
  },
  failedStat: {
    color: "#991B1B",
  },
  pendingStat: {
    color: "#92400E",
  },
  list: {
    borderTopWidth: 1,
    borderTopColor: "#E5E7EB",
    paddingTop: 4,
  },
  emptyText: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#9CA3AF",
    textAlign: "center",
    paddingVertical: 20,
  },
});
