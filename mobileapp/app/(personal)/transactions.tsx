import React, { useEffect, useState, useCallback, useMemo } from "react";
import {
  View,
  Text,
  StyleSheet,
  FlatList,
  TouchableOpacity,
  TextInput,
  ActivityIndicator,
  RefreshControl,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons, Feather } from "@expo/vector-icons";
import { useRouter } from "expo-router";
import { COLORS } from "../../src/constants/colors";
import { useTransactions } from "../../src/hooks/useTransactions";
import { TransactionFilterSheet } from "../../src/components/TransactionFilterSheet";
import { Transaction, TransactionFilters } from "../../src/types/transaction";
import { formatDate } from "../../src/utils/formatting";
import {
  speedUpTransaction,
  getLocalKeypair,
  checkFreighter,
} from "../../src/services/stellarWallet";

// Time threshold to consider a pending transaction stuck (1 minute)
const STUCK_PENDING_MS = 60 * 1000;

// Yield transaction history filtering presets
const DATE_PRESETS: { label: string; days: number }[] = [
  { label: "7D", days: 7 },
  { label: "30D", days: 30 },
  { label: "90D", days: 90 },
  { label: "1Y", days: 365 },
];

function presetStartIso(days: number): string {
  const d = new Date();
  d.setDate(d.getDate() - days);
  d.setHours(0, 0, 0, 0);
  return d.toISOString().slice(0, 10);
}

function isPresetActive(filters: TransactionFilters, days: number): boolean {
  return filters.dateFrom === presetStartIso(days) && !filters.dateTo;
}

function canSpeedUp(tx: Transaction): boolean {
  if (tx.status !== "pending" || !tx.stellarTxXdr) return false;
  return Date.now() - new Date(tx.timestamp).getTime() > STUCK_PENDING_MS;
}

async function resolveWalletSource(): Promise<
  "freighter" | "albedo" | "local"
> {
  const kp = await getLocalKeypair();
  if (kp) return "local";
  if (await checkFreighter()) return "freighter";
  return "local";
}

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
  if (!addr) return "";
  if (addr.length <= 20) return addr;
  return `${addr.slice(0, 8)}…${addr.slice(-6)}`;
}

function hasActiveFilters(f: TransactionFilters): boolean {
  return (
    f.type !== "all" ||
    f.status !== "all" ||
    Boolean(f.dateFrom) ||
    Boolean(f.dateTo) ||
    Boolean(f.amountMin) ||
    Boolean(f.amountMax) ||
    Boolean(f.yieldOnly) ||
    Boolean(f.asset)
  );
}

// Transaction Row with conditional rendering logic based on transaction type
const TransactionRow = React.memo(function TransactionRow({
  item,
  onPress,
  onSpeedUp,
  speedingUp,
}: {
  item: Transaction;
  onPress: () => void;
  onSpeedUp: (tx: Transaction) => void;
  speedingUp: boolean;
}) {
  const isMassPayout = item.type === "mass_payout";
  const isOutgoing = item.type === "sent";
  const isYield = item.type === "yield";
  const icon = typeIcon(item.type);
  const time = formatDate(item.timestamp, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
  const showSpeedUp = canSpeedUp(item);

  return (
    <TouchableOpacity
      style={[styles.txRow, isMassPayout && styles.txRowMassPayout]}
      onPress={onPress}
      activeOpacity={0.75}
      accessibilityRole="button"
      accessibilityLabel={`${item.type} ${item.amount} ${item.asset}`}
      testID={`tx-row-${item.id}`}
    >
      {/* Icon with conditional styling */}
      <View style={[styles.txIcon, { backgroundColor: icon.bg }]}>
        <Ionicons name={icon.name} size={18} color={icon.color} />
      </View>

      {/* Info Column */}
      <View style={styles.txInfo}>
        {/* Label row with conditional SDP disbursement badge */}
        <View style={styles.txLabelRow}>
          <Text style={styles.txLabel} numberOfLines={1}>
            {isMassPayout
              ? item.senderName || item.addressLabel || "Disbursing Organization"
              : item.addressLabel ?? truncateAddress(item.address)}
          </Text>

          {isMassPayout && (
            <View style={styles.sdpBadge} testID="sdp-disbursement-badge">
              <Ionicons name="shield-checkmark" size={10} color="#7C3AED" />
              <Text style={styles.sdpBadgeText}>SDP Disbursement</Text>
            </View>
          )}
        </View>

        {/* Sender information: company and anchor name for SDP mass payouts */}
        {isMassPayout && (
          <View style={styles.senderAnchorRow} testID="sender-anchor-info">
            <Ionicons name="business-outline" size={12} color="#6B7280" />
            <Text style={styles.senderAnchorText} numberOfLines={1}>
              {item.senderAnchor
                ? `Anchor: ${item.senderAnchor}`
                : "Stellar Disbursement Platform"}
            </Text>
          </View>
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

        {showSpeedUp && (
          <TouchableOpacity
            style={styles.speedUpBtn}
            onPress={() => onSpeedUp(item)}
            disabled={speedingUp}
            activeOpacity={0.8}
            accessibilityRole="button"
            accessibilityLabel={`Speed up ${item.amount} ${item.asset}`}
          >
            {speedingUp ? (
              <ActivityIndicator size="small" color="#fff" />
            ) : (
              <>
                <Ionicons
                  name="speedometer-outline"
                  size={13}
                  color="#fff"
                  style={{ marginRight: 5 }}
                />
                <Text style={styles.speedUpBtnText}>Speed up</Text>
              </>
            )}
          </TouchableOpacity>
        )}
      </View>

      {/* Amount Column */}
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

// Section Header components
function SectionHeader({ title }: { title: string }) {
  return (
    <View style={styles.sectionHeader}>
      <Text style={styles.sectionHeaderText}>{title}</Text>
    </View>
  );
}

function MassPayoutSectionHeader({ count }: { count?: number }) {
  return (
    <View style={styles.massPayoutSectionHeader} testID="mass-payout-section-header">
      <View style={styles.massPayoutHeaderLeft}>
        <View style={styles.massPayoutHeaderIconWrap}>
          <Ionicons name="people" size={14} color="#7C3AED" />
        </View>
        <Text style={styles.massPayoutSectionHeaderText}>
          Received Mass Payouts (SDP)
        </Text>
      </View>
      {count !== undefined && count > 0 && (
        <View style={styles.massPayoutCountBadge}>
          <Text style={styles.massPayoutCountText}>{count}</Text>
        </View>
      )}
    </View>
  );
}

// Group transactions with distinct section for Received Mass Payouts
type ListItem =
  | { kind: "mass_payout_header"; key: string; count: number }
  | { kind: "section_header"; key: string; title: string }
  | { kind: "date_header"; key: string; title: string }
  | { kind: "tx"; key: string; tx: Transaction };

function groupTransactions(
  txs: Transaction[],
  sectionFilter: "all" | "mass_payout" | "transfers"
): ListItem[] {
  const result: ListItem[] = [];

  const safeTxs = Array.isArray(txs) ? txs : [];
  const massPayouts = safeTxs.filter((t) => t.type === "mass_payout");
  const standardTransfers = safeTxs.filter((t) => t.type !== "mass_payout");

  // If filtering to mass payouts only
  if (sectionFilter === "mass_payout") {
    if (massPayouts.length > 0) {
      result.push({
        kind: "mass_payout_header",
        key: "mp_main_header",
        count: massPayouts.length,
      });
      for (const tx of massPayouts) {
        result.push({ kind: "tx", key: tx.id, tx });
      }
    }
    return result;
  }

  // If filtering to standard transfers only
  if (sectionFilter === "transfers") {
    let lastDate = "";
    for (const tx of standardTransfers) {
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
        result.push({ kind: "date_header", key: `hdr_${label}`, title: label });
      }
      result.push({ kind: "tx", key: tx.id, tx });
    }
    return result;
  }

  // Default "all" mode: Group Received Mass Payouts in a distinct prominent section,
  // followed by Standard Transfers grouped by date.
  if (massPayouts.length > 0) {
    result.push({
      kind: "mass_payout_header",
      key: "mp_main_header",
      count: massPayouts.length,
    });
    for (const tx of massPayouts) {
      result.push({ kind: "tx", key: tx.id, tx });
    }
  }

  if (standardTransfers.length > 0) {
    result.push({
      kind: "section_header",
      key: "sec_standard_transfers",
      title: "Standard Transfers",
    });

    let lastDate = "";
    for (const tx of standardTransfers) {
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
        result.push({ kind: "date_header", key: `hdr_${label}`, title: label });
      }
      result.push({ kind: "tx", key: tx.id, tx });
    }
  }

  return result;
}

// Full transactions screen with Disbursement history section
export default function TransactionsScreen() {
  const router = useRouter();
  const {
    transactions,
    loading,
    refreshing,
    loadingMore,
    filters,
    setFilters,
    refresh,
    loadMore,
  } = useTransactions();

  const [filterVisible, setFilterVisible] = useState(false);
  const [speedingUpId, setSpeedingUpId] = useState<string | null>(null);
  const [speedUpError, setSpeedUpError] = useState<string | null>(null);
  const [sectionFilter, setSectionFilter] = useState<
    "all" | "mass_payout" | "transfers"
  >("all");
  const [searchText, setSearchText] = useState(filters.search);

  // Sync search input with hook filters
  const handleSearch = (text: string) => {
    setSearchText(text);
    setFilters((prev) => ({ ...prev, search: text }));
  };

  const handleSpeedUp = useCallback(async (tx: Transaction) => {
    if (!tx.stellarTxXdr) return;
    setSpeedingUpId(tx.id);
    setSpeedUpError(null);
    try {
      const source = await resolveWalletSource();
      await speedUpTransaction(tx.stellarTxXdr, source);
      await refresh();
    } catch (err) {
      setSpeedUpError(
        err instanceof Error ? err.message : "Failed to speed up transaction"
      );
    } finally {
      setSpeedingUpId(null);
    }
  }, [refresh]);

  const items = useMemo(
    () => groupTransactions(transactions, sectionFilter),
    [transactions, sectionFilter]
  );

  const filtersActive = hasActiveFilters(filters);

  const renderItem = useCallback(
    ({ item }: { item: ListItem }) => {
      if (item.kind === "mass_payout_header") {
        return <MassPayoutSectionHeader count={item.count} />;
      }
      if (item.kind === "section_header" || item.kind === "date_header") {
        return <SectionHeader title={item.title} />;
      }
      return (
        <TransactionRow
          item={item.tx}
          onPress={() => router.push(`/transaction/${item.tx.id}` as any)}
          onSpeedUp={handleSpeedUp}
          speedingUp={speedingUpId === item.tx.id}
        />
      );
    },
    [router, handleSpeedUp, speedingUpId]
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
            : "Your transactions and mass payouts will appear here."}
        </Text>
      </View>
    );
  };

  return (
    <SafeAreaView style={styles.container} edges={["top"]}>
      {/* Header */}
      <View style={styles.header}>
        <Text style={styles.headerTitle}>Transactions & Disbursements</Text>
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

      {/* Search Bar */}
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
            placeholder="Search address, anchor, memo…"
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

      {/* Section Switcher Tabs */}
      <View style={styles.sectionTabsRow} testID="disbursement-section-tabs">
        <TouchableOpacity
          style={[
            styles.sectionTabBtn,
            sectionFilter === "all" && styles.sectionTabBtnActive,
          ]}
          onPress={() => setSectionFilter("all")}
          accessibilityRole="button"
        >
          <Text
            style={[
              styles.sectionTabText,
              sectionFilter === "all" && styles.sectionTabTextActive,
            ]}
          >
            All Activity
          </Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={[
            styles.sectionTabBtn,
            sectionFilter === "mass_payout" && styles.sectionTabBtnActive,
          ]}
          onPress={() => setSectionFilter("mass_payout")}
          accessibilityRole="button"
          testID="mass-payout-tab"
        >
          <Ionicons
            name="shield-checkmark"
            size={13}
            color={sectionFilter === "mass_payout" ? COLORS.secondary : "#7C3AED"}
            style={{ marginRight: 4 }}
          />
          <Text
            style={[
              styles.sectionTabText,
              sectionFilter === "mass_payout" && styles.sectionTabTextActive,
            ]}
          >
            SDP Disbursements
          </Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={[
            styles.sectionTabBtn,
            sectionFilter === "transfers" && styles.sectionTabBtnActive,
          ]}
          onPress={() => setSectionFilter("transfers")}
          accessibilityRole="button"
        >
          <Text
            style={[
              styles.sectionTabText,
              sectionFilter === "transfers" && styles.sectionTabTextActive,
            ]}
          >
            Transfers
          </Text>
        </TouchableOpacity>
      </View>

      {/* Date Presets Row */}
      <View style={styles.presetsRow}>
        {DATE_PRESETS.map(({ label, days }) => {
          const active = isPresetActive(filters, days);
          return (
            <TouchableOpacity
              key={label}
              style={[styles.presetBtn, active && styles.presetBtnActive]}
              onPress={() => {
                setFilters((prev) => ({
                  ...prev,
                  dateFrom: active ? undefined : presetStartIso(days),
                  dateTo: undefined,
                }));
              }}
              accessibilityLabel={`Filter by last ${label}`}
            >
              <Text
                style={[styles.presetBtnText, active && styles.presetBtnTextActive]}
              >
                {label}
              </Text>
            </TouchableOpacity>
          );
        })}
      </View>

      {/* Speed up error banner */}
      {Boolean(speedUpError) && (
        <View style={styles.speedUpErrorBanner}>
          <Ionicons name="alert-circle-outline" size={16} color="#EF4444" />
          <Text style={styles.speedUpErrorText}>{speedUpError}</Text>
          <TouchableOpacity onPress={() => setSpeedUpError(null)}>
            <Ionicons name="close" size={16} color="#EF4444" />
          </TouchableOpacity>
        </View>
      )}

      {/* Main List */}
      <FlatList
        data={items}
        renderItem={renderItem}
        keyExtractor={(item) => item.key}
        ListFooterComponent={ListFooter}
        ListEmptyComponent={ListEmpty}
        refreshControl={
          <RefreshControl
            refreshing={refreshing}
            onRefresh={refresh}
            tintColor={COLORS.primary}
          />
        }
        onEndReached={loadMore}
        onEndReachedThreshold={0.3}
        contentContainerStyle={[
          styles.listContent,
          items.length === 0 && styles.listContentEmpty,
        ]}
      />

      {/* Filter Sheet */}
      <TransactionFilterSheet
        visible={filterVisible}
        onClose={() => setFilterVisible(false)}
        filters={filters}
        onApply={(applied) => setFilters(applied)}
      />
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: "#F9FAFB",
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 20,
    paddingVertical: 14,
    backgroundColor: COLORS.white,
    borderBottomWidth: 1,
    borderBottomColor: "#F3F4F6",
  },
  headerTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  filterBtn: {
    width: 40,
    height: 40,
    borderRadius: 20,
    backgroundColor: "#F3F4F6",
    justifyContent: "center",
    alignItems: "center",
  },
  filterBtnActive: {
    backgroundColor: COLORS.primary,
  },
  filterDot: {
    position: "absolute",
    top: 8,
    right: 8,
    width: 8,
    height: 8,
    borderRadius: 4,
    backgroundColor: "#22C55E",
  },
  searchRow: {
    paddingHorizontal: 16,
    paddingVertical: 10,
    backgroundColor: COLORS.white,
  },
  searchBox: {
    flexDirection: "row",
    alignItems: "center",
    backgroundColor: "#F3F4F6",
    borderRadius: 12,
    paddingHorizontal: 12,
    height: 42,
  },
  searchIcon: {
    marginRight: 8,
  },
  searchInput: {
    flex: 1,
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: COLORS.black,
  },
  clearBtn: {
    padding: 4,
  },
  sectionTabsRow: {
    flexDirection: "row",
    paddingHorizontal: 16,
    paddingVertical: 8,
    backgroundColor: COLORS.white,
    gap: 8,
    borderBottomWidth: 1,
    borderBottomColor: "#F3F4F6",
  },
  sectionTabBtn: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 20,
    backgroundColor: "#F3F4F6",
  },
  sectionTabBtnActive: {
    backgroundColor: COLORS.primary,
  },
  sectionTabText: {
    fontSize: 13,
    fontFamily: "Outfit_500Medium",
    color: "#6B7280",
  },
  sectionTabTextActive: {
    color: COLORS.secondary,
    fontFamily: "Outfit_600SemiBold",
  },
  presetsRow: {
    flexDirection: "row",
    paddingHorizontal: 16,
    paddingVertical: 8,
    gap: 8,
    backgroundColor: COLORS.white,
    borderBottomWidth: 1,
    borderBottomColor: "#F3F4F6",
  },
  presetBtn: {
    paddingHorizontal: 12,
    paddingVertical: 4,
    borderRadius: 16,
    backgroundColor: "#F3F4F6",
  },
  presetBtnActive: {
    backgroundColor: COLORS.secondary,
  },
  presetBtnText: {
    fontSize: 12,
    fontFamily: "Outfit_500Medium",
    color: "#6B7280",
  },
  presetBtnTextActive: {
    color: COLORS.primary,
    fontFamily: "Outfit_600SemiBold",
  },
  listContent: {
    paddingBottom: 40,
  },
  listContentEmpty: {
    flexGrow: 1,
  },
  txRow: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 16,
    paddingVertical: 14,
    backgroundColor: COLORS.white,
    borderBottomWidth: 1,
    borderBottomColor: "#F3F4F6",
  },
  txRowMassPayout: {
    backgroundColor: "#FAFAFE",
  },
  txIcon: {
    width: 40,
    height: 40,
    borderRadius: 20,
    justifyContent: "center",
    alignItems: "center",
    marginRight: 12,
  },
  txInfo: {
    flex: 1,
    marginRight: 8,
  },
  txLabelRow: {
    flexDirection: "row",
    alignItems: "center",
    flexWrap: "wrap",
    gap: 6,
  },
  txLabel: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    maxWidth: "70%",
  },
  sdpBadge: {
    flexDirection: "row",
    alignItems: "center",
    backgroundColor: "#EDE9FE",
    paddingHorizontal: 6,
    paddingVertical: 2,
    borderRadius: 10,
    gap: 3,
  },
  sdpBadgeText: {
    fontSize: 11,
    fontFamily: "Outfit_600SemiBold",
    color: "#7C3AED",
  },
  senderAnchorRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: 4,
    marginTop: 2,
  },
  senderAnchorText: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#6B7280",
  },
  txMeta: {
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
    marginTop: 4,
  },
  txTime: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#9CA3AF",
  },
  yieldTag: {
    flexDirection: "row",
    alignItems: "center",
    backgroundColor: "#F0FDF4",
    paddingHorizontal: 6,
    paddingVertical: 1,
    borderRadius: 6,
    gap: 2,
  },
  yieldTagText: {
    fontSize: 11,
    fontFamily: "Outfit_500Medium",
    color: "#16A34A",
  },
  statusPill: {
    paddingHorizontal: 6,
    paddingVertical: 1,
    borderRadius: 6,
  },
  statusPillText: {
    fontSize: 11,
    fontFamily: "Outfit_500Medium",
    textTransform: "capitalize",
  },
  speedUpBtn: {
    flexDirection: "row",
    alignItems: "center",
    alignSelf: "flex-start",
    backgroundColor: COLORS.primary,
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 8,
    marginTop: 6,
  },
  speedUpBtnText: {
    fontSize: 12,
    fontFamily: "Outfit_500Medium",
    color: COLORS.white,
  },
  txAmount: {
    alignItems: "flex-end",
  },
  txAmountText: {
    fontSize: 15,
    fontFamily: "Outfit_700Bold",
  },
  txFiat: {
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#9CA3AF",
    marginTop: 2,
  },
  sectionHeader: {
    paddingHorizontal: 16,
    paddingVertical: 10,
    backgroundColor: "#F9FAFB",
  },
  sectionHeaderText: {
    fontSize: 12,
    fontFamily: "Outfit_600SemiBold",
    color: "#6B7280",
    textTransform: "uppercase",
    letterSpacing: 0.5,
  },
  massPayoutSectionHeader: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 16,
    paddingVertical: 12,
    backgroundColor: "#F5F3FF",
    borderLeftWidth: 4,
    borderLeftColor: "#7C3AED",
  },
  massPayoutHeaderLeft: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
  },
  massPayoutHeaderIconWrap: {
    width: 24,
    height: 24,
    borderRadius: 12,
    backgroundColor: "#EDE9FE",
    justifyContent: "center",
    alignItems: "center",
  },
  massPayoutSectionHeaderText: {
    fontSize: 13,
    fontFamily: "Outfit_700Bold",
    color: "#7C3AED",
  },
  massPayoutCountBadge: {
    backgroundColor: "#7C3AED",
    paddingHorizontal: 8,
    paddingVertical: 2,
    borderRadius: 10,
  },
  massPayoutCountText: {
    fontSize: 11,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.white,
  },
  speedUpErrorBanner: {
    flexDirection: "row",
    alignItems: "center",
    backgroundColor: "#FEE2E2",
    paddingHorizontal: 16,
    paddingVertical: 8,
    gap: 8,
  },
  speedUpErrorText: {
    flex: 1,
    fontSize: 12,
    fontFamily: "Outfit_400Regular",
    color: "#EF4444",
  },
  footerLoader: {
    paddingVertical: 20,
    alignItems: "center",
  },
  emptyContainer: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
    paddingTop: 80,
    paddingHorizontal: 40,
  },
  emptyTitle: {
    fontSize: 17,
    fontFamily: "Outfit_600SemiBold",
    color: "#374151",
    marginTop: 16,
  },
  emptySubtext: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#9CA3AF",
    textAlign: "center",
    marginTop: 6,
  },
});
