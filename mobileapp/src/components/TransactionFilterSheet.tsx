import React, { useState } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  Modal,
  ScrollView,
  TextInput,
  Platform,
} from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { COLORS } from "../constants/colors";
import { TransactionFilters } from "../types/transaction";

interface Props {
  visible: boolean;
  filters: TransactionFilters;
  onApply: (f: TransactionFilters) => void;
  onClose: () => void;
}

const TYPE_OPTIONS: { label: string; value: TransactionFilters["type"] }[] = [
  { label: "All", value: "all" },
  { label: "Sent", value: "sent" },
  { label: "Received", value: "received" },
];

const STATUS_OPTIONS: { label: string; value: TransactionFilters["status"] }[] =
  [
    { label: "All", value: "all" },
    { label: "Completed", value: "completed" },
    { label: "Pending", value: "pending" },
    { label: "Failed", value: "failed" },
  ];

function ChipGroup<T extends string>({
  options,
  value,
  onChange,
}: {
  options: { label: string; value: T }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <View style={chipStyles.row}>
      {options.map((o) => (
        <TouchableOpacity
          key={o.value}
          style={[chipStyles.chip, value === o.value && chipStyles.chipActive]}
          onPress={() => onChange(o.value)}
          activeOpacity={0.8}
        >
          <Text
            style={[
              chipStyles.chipText,
              value === o.value && chipStyles.chipTextActive,
            ]}
          >
            {o.label}
          </Text>
        </TouchableOpacity>
      ))}
    </View>
  );
}

const chipStyles = StyleSheet.create({
  row: { flexDirection: "row", flexWrap: "wrap", gap: 8 },
  chip: {
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 100,
    borderWidth: 1.5,
    borderColor: "#E0E0E0",
  },
  chipActive: { borderColor: COLORS.primary, backgroundColor: "#F0FDF4" },
  chipText: { fontSize: 14, fontFamily: "Outfit_500Medium", color: "#666" },
  chipTextActive: { color: COLORS.primary },
});

export function TransactionFilterSheet({
  visible,
  filters,
  onApply,
  onClose,
}: Props) {
  const [local, setLocal] = useState<TransactionFilters>(filters);

  const update = <K extends keyof TransactionFilters>(
    key: K,
    value: TransactionFilters[K]
  ) => setLocal((prev) => ({ ...prev, [key]: value }));

  const reset = () => setLocal({ type: "all", status: "all", search: "" });

  const apply = () => {
    onApply(local);
    onClose();
  };

  // Sync when opened
  React.useEffect(() => {
    if (visible) setLocal(filters);
  }, [visible, filters]);

  return (
    <Modal
      visible={visible}
      animationType="slide"
      transparent
      onRequestClose={onClose}
    >
      <View style={styles.backdrop}>
        <TouchableOpacity style={styles.backdropTouch} onPress={onClose} />
        <View style={styles.sheet}>
          {/* Handle */}
          <View style={styles.handle} />

          {/* Title row */}
          <View style={styles.titleRow}>
            <Text style={styles.title}>Filter Transactions</Text>
            <TouchableOpacity onPress={reset}>
              <Text style={styles.resetText}>Reset</Text>
            </TouchableOpacity>
          </View>

          <ScrollView
            showsVerticalScrollIndicator={false}
            contentContainerStyle={styles.body}
          >
            {/* Type */}
            <Text style={styles.sectionLabel}>Type</Text>
            <ChipGroup
              options={TYPE_OPTIONS}
              value={local.type}
              onChange={(v) => update("type", v)}
            />

            {/* Status */}
            <Text style={styles.sectionLabel}>Status</Text>
            <ChipGroup
              options={STATUS_OPTIONS}
              value={local.status}
              onChange={(v) => update("status", v)}
            />

            {/* Date range */}
            <Text style={styles.sectionLabel}>Date From</Text>
            <TextInput
              style={styles.input}
              placeholder="YYYY-MM-DD"
              placeholderTextColor="#bbb"
              value={local.dateFrom ?? ""}
              onChangeText={(v) => update("dateFrom", v || undefined)}
              keyboardType="numbers-and-punctuation"
            />

            <Text style={styles.sectionLabel}>Date To</Text>
            <TextInput
              style={styles.input}
              placeholder="YYYY-MM-DD"
              placeholderTextColor="#bbb"
              value={local.dateTo ?? ""}
              onChangeText={(v) => update("dateTo", v || undefined)}
              keyboardType="numbers-and-punctuation"
            />

            {/* Amount range */}
            <Text style={styles.sectionLabel}>Amount Range</Text>
            <View style={styles.amountRow}>
              <TextInput
                style={[styles.input, styles.amountInput]}
                placeholder="Min"
                placeholderTextColor="#bbb"
                value={local.amountMin ?? ""}
                onChangeText={(v) => update("amountMin", v || undefined)}
                keyboardType="decimal-pad"
              />
              <Text style={styles.amountSep}>–</Text>
              <TextInput
                style={[styles.input, styles.amountInput]}
                placeholder="Max"
                placeholderTextColor="#bbb"
                value={local.amountMax ?? ""}
                onChangeText={(v) => update("amountMax", v || undefined)}
                keyboardType="decimal-pad"
              />
            </View>
          </ScrollView>

          {/* Apply */}
          <View style={styles.footer}>
            <TouchableOpacity
              style={styles.applyBtn}
              onPress={apply}
              activeOpacity={0.8}
            >
              <Ionicons
                name="checkmark"
                size={18}
                color={COLORS.secondary}
                style={{ marginRight: 6 }}
              />
              <Text style={styles.applyBtnText}>Apply Filters</Text>
            </TouchableOpacity>
          </View>
        </View>
      </View>
    </Modal>
  );
}

const styles = StyleSheet.create({
  backdrop: { flex: 1, justifyContent: "flex-end" },
  backdropTouch: {
    ...StyleSheet.absoluteFillObject,
    backgroundColor: "rgba(0,0,0,0.4)",
  },
  sheet: {
    backgroundColor: COLORS.white,
    borderTopLeftRadius: 24,
    borderTopRightRadius: 24,
    paddingBottom: Platform.OS === "ios" ? 40 : 24,
    maxHeight: "85%",
  },
  handle: {
    width: 40,
    height: 4,
    borderRadius: 2,
    backgroundColor: "#E0E0E0",
    alignSelf: "center",
    marginTop: 12,
    marginBottom: 8,
  },
  titleRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    paddingHorizontal: 20,
    paddingVertical: 12,
    borderBottomWidth: 1,
    borderBottomColor: "#F0F0F0",
  },
  title: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  resetText: {
    fontSize: 14,
    fontFamily: "Outfit_500Medium",
    color: "#EF4444",
  },
  body: { paddingHorizontal: 20, paddingTop: 16, gap: 8, paddingBottom: 8 },
  sectionLabel: {
    fontSize: 13,
    fontFamily: "Outfit_600SemiBold",
    color: "#999",
    textTransform: "uppercase",
    letterSpacing: 0.5,
    marginTop: 12,
    marginBottom: 6,
  },
  input: {
    borderWidth: 1.5,
    borderColor: "#E0E0E0",
    borderRadius: 12,
    paddingHorizontal: 14,
    paddingVertical: 10,
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: COLORS.black,
  },
  amountRow: { flexDirection: "row", alignItems: "center", gap: 8 },
  amountInput: { flex: 1 },
  amountSep: {
    fontSize: 16,
    color: "#999",
    fontFamily: "Outfit_400Regular",
  },
  footer: { paddingHorizontal: 20, paddingTop: 16 },
  applyBtn: {
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    paddingVertical: 16,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
  },
  applyBtnText: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.secondary,
  },
});
