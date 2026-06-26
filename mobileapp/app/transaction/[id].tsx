import React, { useEffect, useState, useCallback } from "react";
import {
  View,
  Text,
  StyleSheet,
  ScrollView,
  TouchableOpacity,
  ActivityIndicator,
  Linking,
  Platform,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useLocalSearchParams, useRouter, Stack } from "expo-router";
import * as Clipboard from "expo-clipboard";
import { COLORS } from "../../src/constants/colors";
import { fetchTransactionById } from "../../src/services/transactionService";
import { shareReceipt } from "../../src/utils/receiptGenerator";
import { formatDate } from "../../src/utils/formatting";
import { Transaction } from "../../src/types/transaction";

const STELLAR_EXPLORER = "https://stellar.expert/explorer/public/tx/";

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

function typeIcon(
  type: Transaction["type"]
): "arrow-up" | "arrow-down" | "swap-horizontal" | "card" {
  switch (type) {
    case "sent":
      return "arrow-up";
    case "received":
      return "arrow-down";
    case "swap":
      return "swap-horizontal";
    default:
      return "card";
  }
}

function typeColor(type: Transaction["type"]): string {
  return type === "received" ? "#22C55E" : COLORS.primary;
}

// ── Sub-components ────────────────────────────────────────────────────────────

function DetailRow({
  label,
  value,
  mono = false,
  copyable = false,
  onCopy,
}: {
  label: string;
  value: string;
  mono?: boolean;
  copyable?: boolean;
  onCopy?: () => void;
}) {
  return (
    <View style={styles.detailRow}>
      <Text style={styles.detailLabel}>{label}</Text>
      <View style={styles.detailValueRow}>
        <Text
          style={[styles.detailValue, mono && styles.detailValueMono]}
          selectable
        >
          {value}
        </Text>
        {copyable && (
          <TouchableOpacity
            onPress={onCopy}
            style={styles.copyBtn}
            accessibilityLabel={`Copy ${label}`}
          >
            <Ionicons name="copy-outline" size={16} color="#999" />
          </TouchableOpacity>
        )}
      </View>
    </View>
  );
}

// ── Main Screen ───────────────────────────────────────────────────────────────

export default function TransactionDetailScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const router = useRouter();
  const [tx, setTx] = useState<Transaction | null>(null);
  const [loading, setLoading] = useState(true);
  const [sharing, setSharing] = useState(false);

  useEffect(() => {
    if (!id) return;
    fetchTransactionById(id).then((t) => {
      setTx(t);
      setLoading(false);
    });
  }, [id]);

  const copy = useCallback(async (text: string, label: string) => {
    await Clipboard.setStringAsync(text);
    if (global.toast) global.toast.success(`${label} copied`);
  }, []);

  const openExplorer = useCallback(() => {
    if (!tx?.stellarTxHash) return;
    Linking.openURL(`${STELLAR_EXPLORER}${tx.stellarTxHash}`);
  }, [tx]);

  const handleShare = useCallback(async () => {
    if (!tx) return;
    setSharing(true);
    try {
      await shareReceipt(tx);
    } catch {
      if (global.toast) global.toast.error("Could not share receipt");
    } finally {
      setSharing(false);
    }
  }, [tx]);

  if (loading) {
    return (
      <SafeAreaView style={styles.container} edges={["top"]}>
        <Stack.Screen options={{ headerShown: false }} />
        <View style={styles.center}>
          <ActivityIndicator size="large" color={COLORS.primary} />
        </View>
      </SafeAreaView>
    );
  }

  if (!tx) {
    return (
      <SafeAreaView style={styles.container} edges={["top"]}>
        <Stack.Screen options={{ headerShown: false }} />
        <View style={styles.header}>
          <TouchableOpacity
            onPress={() => router.back()}
            style={styles.backBtn}
          >
            <Ionicons name="arrow-back" size={24} color={COLORS.black} />
          </TouchableOpacity>
          <Text style={styles.headerTitle}>Receipt</Text>
          <View style={styles.headerSpacer} />
        </View>
        <View style={styles.center}>
          <Text style={styles.notFoundText}>Transaction not found</Text>
        </View>
      </SafeAreaView>
    );
  }

  const date = formatDate(tx.timestamp, {
    year: "numeric",
    month: "long",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });

  const isOutgoing = tx.type === "sent";
  const amountPrefix = isOutgoing ? "−" : "+";

  return (
    <SafeAreaView style={styles.container} edges={["top"]}>
      <Stack.Screen options={{ headerShown: false }} />

      {/* Header */}
      <View style={styles.header}>
        <TouchableOpacity onPress={() => router.back()} style={styles.backBtn}>
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Receipt</Text>
        <TouchableOpacity
          onPress={handleShare}
          style={styles.shareHeaderBtn}
          disabled={sharing}
          accessibilityLabel="Share receipt"
        >
          {sharing ? (
            <ActivityIndicator size="small" color={COLORS.primary} />
          ) : (
            <Ionicons name="share-outline" size={22} color={COLORS.primary} />
          )}
        </TouchableOpacity>
      </View>

      <ScrollView
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        {/* Amount hero */}
        <View style={styles.heroCard}>
          <View
            style={[
              styles.typeIconCircle,
              { backgroundColor: typeColor(tx.type) + "18" },
            ]}
          >
            <Ionicons
              name={typeIcon(tx.type)}
              size={28}
              color={typeColor(tx.type)}
            />
          </View>

          <Text style={[styles.amountHero, { color: typeColor(tx.type) }]}>
            {amountPrefix}
            {tx.amount} {tx.asset}
          </Text>
          <Text style={styles.fiatHero}>
            {tx.fiatValue} {tx.fiatCurrency}
          </Text>

          <View
            style={[
              styles.statusBadge,
              { backgroundColor: statusColor(tx.status) + "18" },
            ]}
          >
            <View
              style={[
                styles.statusDot,
                { backgroundColor: statusColor(tx.status) },
              ]}
            />
            <Text
              style={[styles.statusText, { color: statusColor(tx.status) }]}
            >
              {tx.status.charAt(0).toUpperCase() + tx.status.slice(1)}
            </Text>
          </View>
        </View>

        {/* Details card */}
        <View style={styles.detailCard}>
          <Text style={styles.sectionTitle}>Transaction Details</Text>

          <DetailRow label="Date" value={date} />
          <DetailRow
            label="Type"
            value={tx.type.charAt(0).toUpperCase() + tx.type.slice(1)}
          />
          <DetailRow
            label={isOutgoing ? "To" : "From"}
            value={tx.addressLabel ?? tx.address}
          />
          {tx.addressLabel && (
            <DetailRow
              label="Address"
              value={tx.address}
              mono
              copyable
              onCopy={() => copy(tx.address, "Address")}
            />
          )}
          {tx.memo && <DetailRow label="Memo" value={tx.memo} />}
          {tx.fee && (
            <DetailRow
              label="Network Fee"
              value={`${tx.fee} ${tx.feeAsset ?? ""}`}
            />
          )}
          {tx.network && <DetailRow label="Network" value={tx.network} />}
        </View>

        {/* Stellar hash card */}
        {tx.stellarTxHash && (
          <View style={styles.detailCard}>
            <Text style={styles.sectionTitle}>Blockchain</Text>

            <DetailRow
              label="Transaction Hash"
              value={`${tx.stellarTxHash.slice(0, 16)}…${tx.stellarTxHash.slice(-8)}`}
              mono
              copyable
              onCopy={() => copy(tx.stellarTxHash!, "Transaction hash")}
            />

            <TouchableOpacity
              style={styles.explorerBtn}
              onPress={openExplorer}
              activeOpacity={0.8}
            >
              <Ionicons
                name="open-outline"
                size={16}
                color={COLORS.primary}
                style={{ marginRight: 6 }}
              />
              <Text style={styles.explorerBtnText}>
                View on Stellar Explorer
              </Text>
            </TouchableOpacity>
          </View>
        )}

        {/* Share receipt button */}
        <TouchableOpacity
          style={styles.shareBtn}
          onPress={handleShare}
          disabled={sharing}
          activeOpacity={0.8}
        >
          {sharing ? (
            <ActivityIndicator color={COLORS.secondary} />
          ) : (
            <>
              <Ionicons
                name="share-social-outline"
                size={20}
                color={COLORS.secondary}
                style={{ marginRight: 8 }}
              />
              <Text style={styles.shareBtnText}>Share Receipt</Text>
            </>
          )}
        </TouchableOpacity>
      </ScrollView>
    </SafeAreaView>
  );
}

// ── Styles ────────────────────────────────────────────────────────────────────

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: COLORS.white },
  center: { flex: 1, justifyContent: "center", alignItems: "center" },
  notFoundText: {
    fontFamily: "Outfit_400Regular",
    fontSize: 16,
    color: "#999",
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 20,
    paddingVertical: 15,
  },
  backBtn: {
    width: 40,
    height: 40,
    borderRadius: 20,
    justifyContent: "center",
    alignItems: "center",
  },
  headerTitle: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  headerSpacer: { width: 40 },
  shareHeaderBtn: {
    width: 40,
    height: 40,
    borderRadius: 20,
    justifyContent: "center",
    alignItems: "center",
  },
  scrollContent: {
    paddingHorizontal: 20,
    paddingBottom: Platform.OS === "ios" ? 40 : 24,
    gap: 16,
  },
  // Hero
  heroCard: {
    backgroundColor: "#FAFAFA",
    borderRadius: 24,
    padding: 28,
    alignItems: "center",
    borderWidth: 1,
    borderColor: "#F0F0F0",
  },
  typeIconCircle: {
    width: 64,
    height: 64,
    borderRadius: 32,
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 16,
  },
  amountHero: {
    fontSize: 32,
    fontFamily: "Outfit_700Bold",
    marginBottom: 4,
  },
  fiatHero: {
    fontSize: 16,
    fontFamily: "Outfit_400Regular",
    color: "#666",
    marginBottom: 16,
  },
  statusBadge: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 14,
    paddingVertical: 6,
    borderRadius: 100,
    gap: 6,
  },
  statusDot: { width: 8, height: 8, borderRadius: 4 },
  statusText: { fontSize: 13, fontFamily: "Outfit_600SemiBold" },
  // Detail card
  detailCard: {
    backgroundColor: COLORS.white,
    borderRadius: 20,
    padding: 20,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    gap: 4,
  },
  sectionTitle: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: "#999",
    marginBottom: 8,
    textTransform: "uppercase",
    letterSpacing: 0.5,
  },
  detailRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "flex-start",
    paddingVertical: 10,
    borderBottomWidth: 1,
    borderBottomColor: "#F8F8F8",
  },
  detailLabel: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "#999",
    flex: 1,
  },
  detailValueRow: {
    flex: 2,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: 6,
  },
  detailValue: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    textAlign: "right",
    flexShrink: 1,
  },
  detailValueMono: {
    fontFamily: "Outfit_400Regular",
    fontSize: 12,
  },
  copyBtn: { padding: 4 },
  // Explorer
  explorerBtn: {
    flexDirection: "row",
    alignItems: "center",
    marginTop: 12,
    paddingVertical: 12,
    paddingHorizontal: 16,
    borderRadius: 100,
    borderWidth: 1.5,
    borderColor: COLORS.primary,
    alignSelf: "flex-start",
  },
  explorerBtnText: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
  // Share
  shareBtn: {
    backgroundColor: COLORS.primary,
    borderRadius: 100,
    paddingVertical: 16,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    marginTop: 4,
  },
  shareBtnText: {
    fontSize: 16,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.secondary,
  },
});
