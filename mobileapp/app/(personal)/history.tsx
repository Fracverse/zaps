import React, { useEffect, useState, useCallback } from "react";
import {
  View,
  Text,
  StyleSheet,
  FlatList,
  TouchableOpacity,
  TextInput,
  ActivityIndicator,
  RefreshControl,
  Platform,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons, Feather } from "@expo/vector-icons";
import { useRouter } from "expo-router";
import { COLORS } from "../../src/constants/colors";
import { useTransactions } from "../../src/hooks/useTransactions";
import { TransactionFilterSheet } from "../../src/components/TransactionFilterSheet";
import { Transaction, TransactionFilters } from "../../src/types/transaction";
import { formatDate } from "../../src/utils/formatting";

// ── Helpers ───────────────────────────────────────────────────────────────────

function statusColor(status: Transaction["status"]): string {
  switch (status) {
    case "completed":
      return "#22C55E";
    case "pending":
      return "#F59E0B";
    case "failed":
      return "#EF4444";
  }
}

function typeIcon(type: Transaction["type"]) {
  switch (type) {
    case "sent":
      return { name: "arrow-up" as const, bg: "#FFEBEE", color: "#EF4444" };
    case "received":
      return { name: "arrow-down" as const, bg: "#E8F5E9", color: "#22C55E" };
    case "yield":
      return { name: "trending-up" as const, bg: "#F0FDF4", color: "#16A34A" };
    case "mass_payout":
      return {
        name: "people" as const,
        bg: "#EDE9FE",
        color: "#7C3AED",
      };
    default:
      return {
        name: "swap-horizontal" as const,
        bg: "#E3F2FD",
        color: "#2196F3",
      };
  }
}

function yieldTagLabel(yieldType: Transaction["yieldType"]): string {
  switch (yieldType) {
    case "interest":
      return "Interest";
    case "reward":
      return "Reward";
    case "apy":
      return "APY";
    default:
      return "Yield";
  }
}

function truncateAddress(addr: string): string {
  if (addr.length <= 20) return addr;
  return `${addr.slice(0, 8)}…${addr.slice(-6)}`;
}

function hasActiveFilters(f: TransactionFilters): boolean {
  return (
    f.type !== "all" ||
    f.status !== "all" ||
    !!f.dateFrom ||
    !!f.dateTo ||
    !!f.amountMin ||
    !!f.amountMax ||
    !!f.yieldOnly
  );
}

// ── Transaction row ───────────────────────────────────────────────────────────

const TransactionRow = React.memo(function TransactionRow({
  item,
  onPress,
}: {
  item: Transaction;
  onPress: () => void;
}) {
  const icon = typeIcon(item.type);
  const isOutgoing = item.type === "sent";
  const isYield = item.type === "yield";
  const isMassPayout = item.type === "mass_payout";
  const label = item.addressLabel ?? truncateAddress(item.address);
  const time = formatDate(item.timestamp, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });

  return (
    <TouchableOpacity
      style={styles.txRow}
      onPress={onPress}
      activeOpacity={0.75}
      accessibilityRole="button"
      accessibilityLabel={`${item.type} ${item.amount} ${item.asset}`}
    >
      {/* Icon */}
      <View style={[styles.txIcon, { backgroundColor: icon.bg }]}>
        <Ionicons name={icon.name} size={18} color={icon.color} />
      </View>

      {/* Info */}
      <View style={styles.txInfo}>
        {/* Label row with optional Mass Payout badge */}
        <View style={styles.txLabelRow}>
          <Text style={styles.txLabel} numberOfLines={1}>
            {isMassPayout && item.senderName ? item.senderName : label}
          </Text>
          {isMassPayout && (
            <View style={styles.massPayoutBadge}>
              <Ionicons name="people" size={10} color="#7C3AED" />
              <Text style={styles.massPayoutBadgeText}>Mass Payout</Text>
            </View>
          )}
        </View>
        {/* Anchor/sender info for SDP disbursements */}
        {isMassPayout && item.senderAnchor && (
          <Text style={styles.senderAnchor} numberOfLines={1}>
            via {item.senderAnchor}
          </Text>
        )}
        <View style={styles.txMeta}>
          <Text style={styles.txTime}>{time}</Text>
          {isYield && (
            <View style={styles.yieldTag}>
              <Ionicons name="trending-up" size={10} color="#16A34A" />
              <Text style={styles.yieldTagText}>
                {yieldTagLabel(item.yieldType)}
              </Text>
            </View>
          )}
          {item.status !== "completed" && (
            <View
              style={[
                styles.statusPill,
                { backgroundColor: statusColor(item.status) + "20" },
              ]}
            >
              <Text
                style={[
                  styles.statusPillText,
                  { color: statusColor(item.status) },
                ]}
              >
                {item.status}
              </Text>
            </View>
          )}
        </View>
      </View>

      {/* Amount */}
      <View style={styles.txAmount}>
        <Text
          style={[
            styles.txAmountText,
            {
              color: isOutgoing
                ? COLORS.black
                : isMassPayout
                  ? "#7C3AED"
                  : "#22C55E",
            },
          ]}
        >
          {isOutgoing ? "−" : "+"}
          {item.amount} {item.asset}
        </Text>
        <Text style={styles.txFiat}>
          {item.fiatValue} {item.fiatCurrency}
        </Text>
      </View>
    </TouchableOpacity>
  );
});

// ── Section header ────────────────────────────────────────────────────────────

function SectionHeader({ title }: { title: string }) {
  return (
    <View style={styles.sectionHeader}>
      <Text style={styles.sectionHeaderText}>{title}</Text>
    </View>
  );
}

function MassPayoutSectionHeader() {
  return (
    <View style={styles.massPayoutSectionHeader}>
      <Ionicons name="people" size={13} color="#7C3AED" />
      <Text style={styles.massPayoutSectionHeaderText}>Mass Payouts</Text>
    </View>
  );
}

// ── Group transactions by date ────────────────────────────────────────────────

type ListItem =
  | { kind: "header"; key: string; title: string }
  | { kind: "mass_payout_header"; key: string }
  | { kind: "tx"; key: string; tx: Transaction };

function groupByDate(txs: Transaction[]): ListItem[] {
  const result: ListItem[] = [];
  let lastDate = "";
  let massPayoutHeaderShownForDate = false;

  for (const tx of txs) {
    const d = new Date(tx.timestamp);
    const today = new Date();
    const yesterday = new Date(today);
    yesterday.setDate(today.getDate() - 1);

    let label: string;
    if (d.toDateString() === today.toDateString()) {
      label = "Today";
    } else if (d.toDateString() === yesterday.toDateString()) {
      label = "Yesterday";
    } else {
      label = formatDate(tx.timestamp, {
        month: "long",
        day: "numeric",
        year: "numeric",
      });
    }

    if (label !== lastDate) {
      lastDate = label;
      massPayoutHeaderShownForDate = false;
      result.push({ kind: "header", key: `hdr_${label}`, title: label });
    }

    // Insert a "Mass Payouts" sub-section header the first time we encounter
    // a mass_payout transaction within each date group.
    if (tx.type === "mass_payout" && !massPayoutHeaderShownForDate) {
      massPayoutHeaderShownForDate = true;
      result.push({ kind: "mass_payout_header", key: `mp_hdr_${label}` });
    }

    result.push({ kind: "tx", key: tx.id, tx });
  }

  return result;
}

// ── Main Screen ───────────────────────────────────────────────────────────────

export default function HistoryScreen() {
  const router = useRouter();
  const {
    items,
    loading,
    refreshing,
    loadingMore,
    error,
    filters,
    total,
    reload,
    refresh,
    loadMore,
    applyFilters,
  } = useTransactions();

  const [searchText, setSearchText] = useState("");
  const [filterVisible, setFilterVisible] = useState(false);
  const [searchDebounce, setSearchDebounce] = useState<ReturnType<
    typeof setTimeout
  > | null>(null);

  // Initial load
  useEffect(() => {
    reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Debounced search
  const handleSearch = useCallback(
    (text: string) => {
      setSearchText(text);
      if (searchDebounce) clearTimeout(searchDebounce);
      const t = setTimeout(() => {
        applyFilters({ ...filters, search: text });
      }, 350);
      setSearchDebounce(t);
    },
    [filters, applyFilters, searchDebounce]
  );

  const handleApplyFilters = useCallback(
    (f: TransactionFilters) => {
      applyFilters({ ...f, search: searchText });
    },
    [applyFilters, searchText]
  );

  const listData = groupByDate(items);
  const filtersActive = hasActiveFilters(filters);

  const renderItem = useCallback(
    ({ item }: { item: ListItem }) => {
      if (item.kind === "header") {
        return <SectionHeader title={item.title} />;
      }
      if (item.kind === "mass_payout_header") {
        return <MassPayoutSectionHeader />;
      }
      return (
        <TransactionRow
          item={item.tx}
          onPress={() => router.push(`/transaction/${item.tx.id}` as any)}
        />
      );
    },
    [router]
  );

  const ListFooter = () => {
    if (!loadingMore) return null;
    return (
      <View style={styles.footerLoader}>
        <ActivityIndicator size="small" color={COLORS.primary} />
      </View>
    );
  };

  const ListEmpty = () => {
    if (loading) return null;
    return (
      <View style={styles.emptyContainer}>
        <Feather name="inbox" size={48} color="#ccc" />
        <Text style={styles.emptyTitle}>No transactions</Text>
        <Text style={styles.emptySubtext}>
          {filtersActive || searchText
            ? "Try adjusting your filters or search."
            : "Your transactions will appear here."}
        </Text>
      </View>
    );
  };

  return (
    <SafeAreaView style={styles.container} edges={["top"]}>
      {/* Header */}
      <View style={styles.header}>
        <Text style={styles.headerTitle}>History</Text>
        <TouchableOpacity
          style={[styles.filterBtn, filtersActive && styles.filterBtnActive]}
          onPress={() => setFilterVisible(true)}
          accessibilityLabel="Open filters"
        >
          <Ionicons
            name="options-outline"
            size={20}
            color={filtersActive ? COLORS.secondary : COLORS.primary}
          />
          {filtersActive && <View style={styles.filterDot} />}
        </TouchableOpacity>
      </View>

      {/* Search bar */}
      <View style={styles.searchRow}>
        <View style={styles.searchBox}>
          <Ionicons
            name="search-outline"
            size={18}
            color="#999"
            style={styles.searchIcon}
          />
          <TextInput
            style={styles.searchInput}
            placeholder="Search address, memo, hash…"
            placeholderTextColor="#bbb"
            value={searchText}
            onChangeText={handleSearch}
            autoCapitalize="none"
            autoCorrect={false}
            returnKeyType="search"
          />
          {searchText.length > 0 && (
            <TouchableOpacity
              onPress={() => handleSearch("")}
              style={styles.clearBtn}
            >
              <Ionicons name="close-circle" size={16} color="#bbb" />
            </TouchableOpacity>
          )}
        </View>
      </View>

      {/* Quick filters row — Yield only & Mass Payouts */}
      <View style={styles.quickFilterRow}>
        <TouchableOpacity
          style={[
            styles.quickFilterChip,
            filters.yieldOnly && styles.quickFilterChipActive,
          ]}
          onPress={() =>
            applyFilters({
              ...filters,
              yieldOnly: !filters.yieldOnly,
              search: searchText,
            })
          }
          activeOpacity={0.8}
          accessibilityRole="switch"
          accessibilityLabel="Filter by yield only"
          accessibilityState={{ checked: !!filters.yieldOnly }}
        >
          <Ionicons
            name="trending-up"
            size={14}
            color={filters.yieldOnly ? COLORS.secondary : "#16A34A"}
            style={{ marginRight: 6 }}
          />
          <Text
            style={[
              styles.quickFilterChipText,
              filters.yieldOnly && styles.quickFilterChipTextActive,
            ]}
          >
            Yield only
          </Text>
        </TouchableOpacity>

        {/* Quick filter — Mass Payouts (SDP disbursements) */}
        <TouchableOpacity
          style={[
            styles.quickFilterChipMassPayout,
            filters.type === "mass_payout" &&
              styles.quickFilterChipMassPayoutActive,
          ]}
          onPress={() =>
            applyFilters({
              ...filters,
              type: filters.type === "mass_payout" ? "all" : "mass_payout",
              yieldOnly: false,
              search: searchText,
            })
          }
          activeOpacity={0.8}
          accessibilityRole="switch"
          accessibilityLabel="Filter by mass payouts"
          accessibilityState={{ checked: filters.type === "mass_payout" }}
        >
          <Ionicons
            name="people"
            size={14}
            color={
              filters.type === "mass_payout" ? COLORS.secondary : "#7C3AED"
            }
            style={{ marginRight: 6 }}
          />
          <Text
            style={[
              styles.quickFilterChipMassPayoutText,
              filters.type === "mass_payout" &&
                styles.quickFilterChipMassPayoutTextActive,
            ]}
          >
            Mass Payouts
          </Text>
        </TouchableOpacity>
      </View>

      {/* Summary row */}
      {!loading && items.length > 0 && (
        <View style={styles.summaryRow}>
          <Text style={styles.summaryText}>
            {total} transaction{total !== 1 ? "s" : ""}
            {filtersActive || searchText ? " (filtered)" : ""}
          </Text>
        </View>
      )}

      {/* Loading skeleton */}
      {loading && (
        <View style={styles.loadingContainer}>
          <ActivityIndicator size="large" color={COLORS.primary} />
          <Text style={styles.loadingText}>Loading transactions…</Text>
        </View>
      )}

      {/* Error */}
      {error && !loading && (
        <View style={styles.errorContainer}>
          <Ionicons name="cloud-offline-outline" size={40} color="#ccc" />
          <Text style={styles.errorText}>{error}</Text>
          <TouchableOpacity style={styles.retryBtn} onPress={() => reload()}>
            <Text style={styles.retryBtnText}>Retry</Text>
          </TouchableOpacity>
        </View>
      )}

      {/* List */}
      {!loading && !error && (
        <FlatList
          data={listData}
          keyExtractor={(item) => item.key}
          renderItem={renderItem}
          contentContainerStyle={styles.listContent}
          showsVerticalScrollIndicator={false}
          initialNumToRender={10}
          windowSize={5}
          removeClippedSubviews
          refreshControl={
            <RefreshControl
              refreshing={refreshing}
              onRefresh={refresh}
              tintColor={COLORS.primary}
              colors={[COLORS.primary]}
            />
          }
          onEndReached={loadMore}
          onEndReachedThreshold={0.3}
          ListFooterComponent={<ListFooter />}
          ListEmptyComponent={<ListEmpty />}
        />
      )}

      {/* Filter sheet */}
      <TransactionFilterSheet
        visible={filterVisible}
        filters={filters}
        onApply={handleApplyFilters}
        onClose={() => setFilterVisible(false)}
      />
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
    paddingTop: 8,
    paddingBottom: 4,
  },
  headerTitle: {
    fontSize: 24,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  filterBtn: {
    width: 40,
    height: 40,
    borderRadius: 20,
    borderWidth: 1.5,
    borderColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
  },
  filterBtnActive: {
    backgroundColor: COLORS.primary,
  },
  filterDot: {
    position: "absolute",
    top: 6,
    right: 6,
    width: 8,
    height: 8,
    borderRadius: 4,
    backgroundColor: COLORS.secondary,
  },
  // Search
  searchRow: { paddingHorizontal: 20, paddingVertical: 10 },
  searchBox: {
    flexDirection: "row",
    alignItems: "center",
    backgroundColor: "#F5F5F5",
    borderRadius: 100,
    paddingHorizontal: 14,
    height: 44,
  },
  searchIcon: { marginRight: 8 },
  searchInput: {
    flex: 1,
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: COLORS.black,
  },
  clearBtn: { padding: 4 },
  // Summary
  summaryRow: { paddingHorizontal: 20, paddingBottom: 6 },
  summaryText: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  // Section header
  sectionHeader: {
    paddingHorizontal: 4,
    paddingTop: 16,
    paddingBottom: 6,
  },
  sectionHeaderText: {
    fontSize: 13,
    fontFamily: "Outfit_600SemiBold",
    color: "#999",
    textTransform: "uppercase",
    letterSpacing: 0.5,
  },
  // Mass Payout sub-section header
  massPayoutSectionHeader: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 4,
    paddingTop: 12,
    paddingBottom: 6,
    gap: 5,
  },
  massPayoutSectionHeaderText: {
    fontSize: 12,
    fontFamily: "Outfit_700Bold",
    color: "#7C3AED",
    textTransform: "uppercase",
    letterSpacing: 0.5,
  },
  // Transaction row
  txRow: {
    flexDirection: "row",
    alignItems: "center",
    paddingVertical: 12,
    paddingHorizontal: 14,
    backgroundColor: "#FAFAFA",
    borderRadius: 16,
    marginBottom: 8,
  },
  txIcon: {
    width: 40,
    height: 40,
    borderRadius: 20,
    justifyContent: "center",
    alignItems: "center",
    marginRight: 12,
  },
  txInfo: { flex: 1, marginRight: 8 },
  txLabelRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
    marginBottom: 3,
  },
  txLabel: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    flexShrink: 1,
  },
  massPayoutBadge: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 6,
    paddingVertical: 2,
    borderRadius: 100,
    backgroundColor: "#EDE9FE",
    gap: 3,
  },
  massPayoutBadgeText: {
    fontSize: 9,
    fontFamily: "Outfit_700Bold",
    color: "#7C3AED",
    textTransform: "uppercase",
    letterSpacing: 0.3,
  },
  senderAnchor: {
    fontSize: 11,
    fontFamily: "Outfit_500Medium",
    color: "#7C3AED",
    marginBottom: 3,
  },
  txMeta: { flexDirection: "row", alignItems: "center", gap: 6 },
  txTime: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  statusPill: {
    paddingHorizontal: 8,
    paddingVertical: 2,
    borderRadius: 100,
  },
  statusPillText: {
    fontSize: 11,
    fontFamily: "Outfit_600SemiBold",
    textTransform: "capitalize",
  },
  yieldTag: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 6,
    paddingVertical: 2,
    borderRadius: 100,
    backgroundColor: "#F0FDF4",
    gap: 3,
  },
  yieldTagText: {
    fontSize: 10,
    fontFamily: "Outfit_600SemiBold",
    color: "#16A34A",
  },
  quickFilterRow: {
    flexDirection: "row",
    paddingHorizontal: 20,
    paddingBottom: 8,
    gap: 8,
  },
  quickFilterChip: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 100,
    borderWidth: 1.5,
    borderColor: "#16A34A",
    backgroundColor: "#F0FDF4",
  },
  quickFilterChipActive: {
    backgroundColor: "#16A34A",
  },
  quickFilterChipText: {
    fontSize: 12,
    fontFamily: "Outfit_600SemiBold",
    color: "#16A34A",
  },
  quickFilterChipTextActive: {
    color: COLORS.secondary,
  },
  quickFilterChipMassPayout: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 100,
    borderWidth: 1.5,
    borderColor: "#7C3AED",
    backgroundColor: "#EDE9FE",
  },
  quickFilterChipMassPayoutActive: {
    backgroundColor: "#7C3AED",
  },
  quickFilterChipMassPayoutText: {
    fontSize: 12,
    fontFamily: "Outfit_600SemiBold",
    color: "#7C3AED",
  },
  quickFilterChipMassPayoutTextActive: {
    color: COLORS.secondary,
  },
  txAmount: { alignItems: "flex-end" },
  txAmountText: {
    fontSize: 14,
    fontFamily: "Outfit_700Bold",
  },
  txFiat: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#999",
    marginTop: 2,
  },
  // List
  listContent: {
    paddingHorizontal: 20,
    paddingBottom: Platform.OS === "ios" ? 40 : 24,
  },
  footerLoader: { paddingVertical: 20, alignItems: "center" },
  // Empty
  emptyContainer: {
    alignItems: "center",
    paddingTop: 80,
    gap: 10,
  },
  emptyTitle: {
    fontSize: 18,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
  },
  emptySubtext: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#999",
    textAlign: "center",
    paddingHorizontal: 32,
  },
  // Loading
  loadingContainer: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
    gap: 12,
  },
  loadingText: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  // Error
  errorContainer: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
    gap: 12,
    paddingHorizontal: 32,
  },
  errorText: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    textAlign: "center",
  },
  retryBtn: {
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    paddingVertical: 12,
    paddingHorizontal: 28,
  },
  retryBtnText: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.secondary,
  },
});
