import React, { useState, useMemo, useEffect, useRef } from "react";
import {
  View,
  Text,
  StyleSheet,
  ScrollView,
  TouchableOpacity,
  SafeAreaView,
  FlatList,
  Animated,
  Share,
  Platform,
  ActivityIndicator,
} from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { useLocalSearchParams, useRouter, Stack } from "expo-router";
import { captureRef } from "react-native-view-shot";
import { COLORS } from "../../../src/constants/colors";
import {
  BatchPayoutItem,
  BatchPayoutSummary,
} from "../../../src/types/batchPayout";

type BatchStep = "pending" | "parsing" | "initiating" | "completed";

interface BatchStepMeta {
  key: BatchStep;
  label: string;
  icon: keyof typeof Ionicons.glyphMap;
}

const BATCH_STEPS: BatchStepMeta[] = [
  { key: "pending", label: "Pending", icon: "time-outline" },
  { key: "parsing", label: "Parsing", icon: "document-text-outline" },
  { key: "initiating", label: "Initiating", icon: "rocket-outline" },
  { key: "completed", label: "Completed", icon: "checkmark-circle-outline" },
];

const MOCK_SUMMARY: BatchPayoutSummary = {
  id: "BATCH-001",
  totalAmount: "15000.00",
  currency: "USDC",
  itemCount: 48,
  completedCount: 32,
  failedCount: 3,
  createdAt: new Date(Date.now() - 3600000).toISOString(),
};

const MOCK_ITEMS: BatchPayoutItem[] = Array.from({ length: 48 }, (_, i) => {
  const statuses: BatchPayoutItem["status"][] = [
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "completed",
    "pending",
    "pending",
    "pending",
    "pending",
    "pending",
    "pending",
    "pending",
    "pending",
    "pending",
    "pending",
    "pending",
    "pending",
    "pending",
    "failed",
    "failed",
    "failed",
  ];
  return {
    id: `item-${i + 1}`,
    recipientName: `Recipient ${i + 1}`,
    recipientAddress: `GABCDEF${String(i + 1).padStart(5, "0")}123456789`,
    amount: `${(Math.random() * 500 + 50).toFixed(2)}`,
    currency: "USDC",
    status: statuses[i] ?? "pending",
  };
});

function StepIndicator({ currentStep }: { currentStep: BatchStep }) {
  const currentIndex = BATCH_STEPS.findIndex((s) => s.key === currentStep);

  return (
    <View style={styles.stepContainer}>
      {BATCH_STEPS.map((step, index) => {
        const isActive = index === currentIndex;
        const isPast = index < currentIndex;
        const isFuture = index > currentIndex;

        return (
          <React.Fragment key={step.key}>
            <View style={styles.stepItem}>
              <View
                style={[
                  styles.stepCircle,
                  isActive && styles.stepCircleActive,
                  isPast && styles.stepCirclePast,
                  isFuture && styles.stepCircleFuture,
                ]}
              >
                <Ionicons
                  name={isPast ? "checkmark" : step.icon}
                  size={18}
                  color={isPast ? "#fff" : isActive ? COLORS.primary : "#bbb"}
                />
              </View>
              <Text
                style={[
                  styles.stepLabel,
                  isActive && styles.stepLabelActive,
                  isPast && styles.stepLabelPast,
                  isFuture && styles.stepLabelFuture,
                ]}
              >
                {step.label}
              </Text>
            </View>
            {index < BATCH_STEPS.length - 1 && (
              <View
                style={[
                  styles.stepConnector,
                  (isPast || (isActive && index < currentIndex)) &&
                    styles.stepConnectorActive,
                ]}
              />
            )}
          </React.Fragment>
        );
      })}
    </View>
  );
}

/**
 * AnimatedProgressBar
 *
 * #701 — Batch Payout Progress Bar
 *
 * Animates the progress fill width smoothly using React Native's built-in
 * `Animated` API (the project's existing animation approach — no reanimated).
 *
 * The bar is driven by a 0–100 `progress` value derived from
 * `completedCount / itemCount` on the batch execution state.
 *
 * States handled:
 *  - initial (0 %): bar starts empty, no animation until first update
 *  - partial (0 < n < 100): fill animates to the new width on each update
 *  - completed (100 %): bar snaps to full width with a slightly faster spring
 */
function AnimatedProgressBar({
  progress,
  label,
}: {
  progress: number;
  label: string;
}) {
  const pct = Math.min(100, Math.max(0, progress));
  const isComplete = pct >= 100;

  // Animated value in the 0–100 range so we can interpolate to "X%".
  const animatedPct = useRef(new Animated.Value(0)).current;

  // Track layout width of the track so we can drive the fill in pixels, which
  // Animated supports natively without needing reanimated.
  const [trackWidth, setTrackWidth] = useState<number>(0);

  useEffect(() => {
    // Spring to the target percentage — smooth and snappy at completion.
    Animated.spring(animatedPct, {
      toValue: pct,
      useNativeDriver: false, // layout animations require JS-driven updates
      speed: isComplete ? 20 : 12,
      bounciness: isComplete ? 2 : 4,
    }).start();
  }, [pct, isComplete, animatedPct]);

  // Interpolate from the 0-100 animated value to pixel width.
  const fillWidth =
    trackWidth > 0
      ? animatedPct.interpolate({
          inputRange: [0, 100],
          outputRange: [0, trackWidth],
          extrapolate: "clamp",
        })
      : undefined;

  // Fill colour transitions from primary to secondary green at completion.
  const fillColor = animatedPct.interpolate({
    inputRange: [0, 99, 100],
    outputRange: [COLORS.primary, COLORS.primary, COLORS.secondary],
    extrapolate: "clamp",
  });

  return (
    <View style={styles.progressContainer}>
      <View style={styles.progressHeader}>
        <Text style={styles.progressLabel}>{label}</Text>
        <Text style={styles.progressText}>{pct.toFixed(0)}%</Text>
      </View>
      <View
        style={styles.progressTrack}
        onLayout={(e) => setTrackWidth(e.nativeEvent.layout.width)}
      >
        <Animated.View
          style={[
            styles.progressFill,
            fillWidth !== undefined
              ? { width: fillWidth, backgroundColor: fillColor }
              : { width: `${pct}%` as unknown as number },
          ]}
          accessibilityRole="progressbar"
          accessibilityValue={{ min: 0, max: 100, now: Math.round(pct) }}
        />
      </View>
    </View>
  );
}

function StatusBadge({ status }: { status: BatchPayoutItem["status"] }) {
  const colorMap: Record<BatchPayoutItem["status"], string> = {
    completed: "#22C55E",
    pending: "#F59E0B",
    failed: "#EF4444",
  };
  const iconMap: Record<
    BatchPayoutItem["status"],
    keyof typeof Ionicons.glyphMap
  > = {
    completed: "checkmark-circle",
    pending: "time",
    failed: "close-circle",
  };

  return (
    <View
      style={[styles.statusBadge, { backgroundColor: colorMap[status] + "18" }]}
    >
      <Ionicons name={iconMap[status]} size={14} color={colorMap[status]} />
      <Text style={[styles.statusText, { color: colorMap[status] }]}>
        {status.charAt(0).toUpperCase() + status.slice(1)}
      </Text>
    </View>
  );
}

function BatchItemRow({ item }: { item: BatchPayoutItem }) {
  return (
    <View style={styles.itemRow}>
      <View style={styles.itemInfo}>
        <Text style={styles.itemName} numberOfLines={1}>
          {item.recipientName}
        </Text>
        <Text style={styles.itemAddress} numberOfLines={1}>
          {item.recipientAddress}
        </Text>
      </View>
      <View style={styles.itemRight}>
        <Text style={styles.itemAmount}>
          {item.amount} {item.currency}
        </Text>
        <StatusBadge status={item.status} />
      </View>
    </View>
  );
}

export default function BatchDetailScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const router = useRouter();
  const [summary] = useState<BatchPayoutSummary>(MOCK_SUMMARY);
  const [items] = useState<BatchPayoutItem[]>(MOCK_ITEMS);
  const [filter, setFilter] = useState<
    "all" | "completed" | "pending" | "failed"
  >("all");
  const [exporting, setExporting] = useState(false);

  const receiptRef = useRef<View>(null);

  const currentStep: BatchStep = useMemo(() => {
    if (summary.failedCount > 0) return "completed";
    if (summary.completedCount === summary.itemCount) return "completed";
    if (summary.completedCount > summary.itemCount / 2) return "initiating";
    if (summary.completedCount > 0) return "parsing";
    return "pending";
  }, [summary]);

  const progress = useMemo(
    () =>
      summary.itemCount > 0
        ? (summary.completedCount / summary.itemCount) * 100
        : 0,
    [summary]
  );

  const filteredItems = useMemo(
    () => (filter === "all" ? items : items.filter((i) => i.status === filter)),
    [items, filter]
  );

  const successRate = useMemo(
    () =>
      summary.itemCount > 0
        ? (
            (summary.completedCount /
              (summary.itemCount - summary.failedCount)) *
            100
          ).toFixed(1)
        : "0",
    [summary]
  );

  /**
   * #694 — Transaction receipt export
   *
   * Captures the hidden receipt container via react-native-view-shot and
   * hands the resulting PNG over to the system share sheet. Works for both
   * completed batches and (once wired to real data) P2P transfers.
   */
  const handleExportReceipt = async () => {
    if (!receiptRef.current) return;
    try {
      setExporting(true);
      const uri = await captureRef(receiptRef, {
        format: "png",
        quality: 1,
        result: "tmpfile",
        width: 680,
      });
      await Share.share({
        url: Platform.OS === "ios" ? uri : `file://${uri}`,
        message: `ZAPS batch receipt #${summary.id}`,
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Could not export receipt";
      (
        globalThis as unknown as {
          toast?: { error: (message: string) => void };
        }
      ).toast?.error(msg);
    } finally {
      setExporting(false);
    }
  };

  const filters = [
    { key: "all" as const, label: "All" },
    { key: "completed" as const, label: "Success" },
    { key: "pending" as const, label: "Pending" },
    { key: "failed" as const, label: "Failed" },
  ];

  return (
    <SafeAreaView style={styles.container} edges={["top"]}>
      <Stack.Screen options={{ headerShown: false }} />

      <View style={styles.header}>
        <TouchableOpacity onPress={() => router.back()} style={styles.backBtn}>
          <Ionicons name="arrow-back" size={24} color={COLORS.black} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Batch Details</Text>
        <TouchableOpacity
          onPress={handleExportReceipt}
          disabled={exporting}
          style={styles.exportBtn}
          accessibilityLabel="Export receipt"
        >
          {exporting ? (
            <ActivityIndicator
              testID="export-spinner"
              size="small"
              color={COLORS.primary}
            />
          ) : (
            <Ionicons name="share-outline" size={22} color={COLORS.primary} />
          )}
        </TouchableOpacity>
      </View>

      <ScrollView
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        <View style={styles.summaryCard}>
          <Text style={styles.batchId}>Batch #{summary.id}</Text>
          <Text style={styles.totalAmount}>
            {summary.totalAmount} {summary.currency}
          </Text>
          <Text style={styles.itemCount}>{summary.itemCount} recipients</Text>
        </View>

        <View style={styles.stepperCard}>
          <Text style={styles.sectionTitle}>Execution Progress</Text>
          <StepIndicator currentStep={currentStep} />
          <AnimatedProgressBar progress={progress} label="Overall Progress" />
        </View>

        <View style={styles.statsCard}>
          <View style={styles.statItem}>
            <Ionicons name="checkmark-circle" size={20} color="#22C55E" />
            <View style={styles.statInfo}>
              <Text style={styles.statValue}>{summary.completedCount}</Text>
              <Text style={styles.statLabel}>Success</Text>
            </View>
          </View>
          <View style={styles.statDivider} />
          <View style={styles.statItem}>
            <Ionicons name="close-circle" size={20} color="#EF4444" />
            <View style={styles.statInfo}>
              <Text style={styles.statValue}>{summary.failedCount}</Text>
              <Text style={styles.statLabel}>Failed</Text>
            </View>
          </View>
          <View style={styles.statDivider} />
          <View style={styles.statItem}>
            <Ionicons name="trending-up" size={20} color="#22C55E" />
            <View style={styles.statInfo}>
              <Text style={styles.statValue}>{successRate}%</Text>
              <Text style={styles.statLabel}>Success Rate</Text>
            </View>
          </View>
        </View>

        <View style={styles.itemsSection}>
          <Text style={styles.sectionTitle}>Recipients</Text>

          <View style={styles.filterRow}>
            {filters.map((f) => (
              <TouchableOpacity
                key={f.key}
                style={[
                  styles.filterChip,
                  filter === f.key && styles.filterChipActive,
                ]}
                onPress={() => setFilter(f.key)}
              >
                <Text
                  style={[
                    styles.filterChipText,
                    filter === f.key && styles.filterChipTextActive,
                  ]}
                >
                  {f.label}
                </Text>
              </TouchableOpacity>
            ))}
          </View>

          {filteredItems.map((item) => (
            <BatchItemRow key={item.id} item={item} />
          ))}
        </View>
      </ScrollView>

      {/* Hidden receipt container — captured via react-native-view-shot (#694).
          Rendered off-screen so it never flashes on device but stays capturable. */}
      <View
        ref={receiptRef}
        collapsable={false}
        pointerEvents="none"
        style={styles.hiddenReceipt}
      >
        <View style={styles.receiptCard}>
          <View style={styles.receiptHeader}>
            <Text style={styles.receiptBrand}>ZAPS</Text>
            <Text style={styles.receiptTitle}>Batch Receipt</Text>
          </View>

          <View style={styles.receiptDivider} />

          <View style={styles.receiptRow}>
            <Text style={styles.receiptLabel}>Batch ID</Text>
            <Text style={styles.receiptValue}>#{summary.id}</Text>
          </View>
          <View style={styles.receiptRow}>
            <Text style={styles.receiptLabel}>Issued</Text>
            <Text style={styles.receiptValue}>
              {new Date(summary.createdAt).toLocaleString()}
            </Text>
          </View>

          <View style={styles.receiptDivider} />

          <View style={styles.receiptTotalRow}>
            <Text style={styles.receiptTotalLabel}>Total Payout</Text>
            <Text style={styles.receiptTotalValue}>
              {summary.totalAmount} {summary.currency}
            </Text>
          </View>

          <View style={styles.receiptRow}>
            <Text style={styles.receiptLabel}>Recipients</Text>
            <Text style={styles.receiptValue}>{summary.itemCount}</Text>
          </View>
          <View style={styles.receiptRow}>
            <Text style={styles.receiptLabel}>Completed</Text>
            <Text style={[styles.receiptValue, { color: "#22C55E" }]}>
              {summary.completedCount}
            </Text>
          </View>
          <View style={styles.receiptRow}>
            <Text style={styles.receiptLabel}>Failed</Text>
            <Text style={[styles.receiptValue, { color: "#EF4444" }]}>
              {summary.failedCount}
            </Text>
          </View>
          <View style={styles.receiptRow}>
            <Text style={styles.receiptLabel}>Success Rate</Text>
            <Text style={styles.receiptValue}>{successRate}%</Text>
          </View>

          <View style={styles.receiptDivider} />

          <Text style={styles.receiptFooter}>Generated by ZAPS mobile app</Text>
        </View>
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: COLORS.white },
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
  scrollContent: {
    paddingHorizontal: 20,
    paddingBottom: 40,
    gap: 16,
  },
  summaryCard: {
    backgroundColor: COLORS.primary,
    borderRadius: 24,
    padding: 24,
    alignItems: "center",
  },
  batchId: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: "rgba(255,255,255,0.7)",
    marginBottom: 8,
  },
  totalAmount: {
    fontSize: 32,
    fontFamily: "Outfit_700Bold",
    color: COLORS.secondary,
    marginBottom: 4,
  },
  itemCount: {
    fontSize: 14,
    fontFamily: "Outfit_400Regular",
    color: "rgba(255,255,255,0.7)",
  },
  stepperCard: {
    backgroundColor: "#FAFAFA",
    borderRadius: 20,
    padding: 20,
    borderWidth: 1,
    borderColor: "#F0F0F0",
  },
  sectionTitle: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: "#999",
    marginBottom: 16,
    textTransform: "uppercase",
    letterSpacing: 0.5,
  },
  stepContainer: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    marginBottom: 20,
  },
  stepItem: { alignItems: "center", flex: 1 },
  stepCircle: {
    width: 36,
    height: 36,
    borderRadius: 18,
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 6,
  },
  stepCircleActive: {
    backgroundColor: COLORS.secondary,
    borderWidth: 2,
    borderColor: COLORS.primary,
  },
  stepCirclePast: { backgroundColor: COLORS.primary },
  stepCircleFuture: { backgroundColor: "#F0F0F0" },
  stepLabel: {
    fontSize: 10,
    fontFamily: "Outfit_500Medium",
    color: "#999",
    textAlign: "center",
  },
  stepLabelActive: { color: COLORS.primary, fontFamily: "Outfit_700Bold" },
  stepLabelPast: { color: COLORS.primary },
  stepLabelFuture: { color: "#bbb" },
  stepConnector: {
    height: 2,
    flex: 1,
    backgroundColor: "#E0E0E0",
    marginBottom: 24,
  },
  stepConnectorActive: { backgroundColor: COLORS.primary },
  progressContainer: { marginTop: 4 },
  progressHeader: {
    flexDirection: "row",
    justifyContent: "space-between",
    marginBottom: 8,
  },
  progressLabel: {
    fontSize: 13,
    fontFamily: "Outfit_500Medium",
    color: "#666",
  },
  progressText: {
    fontSize: 13,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  progressTrack: {
    height: 8,
    backgroundColor: "#E0E0E0",
    borderRadius: 4,
    overflow: "hidden",
  },
  progressFill: {
    height: "100%",
    backgroundColor: COLORS.primary,
    borderRadius: 4,
  },
  statsCard: {
    flexDirection: "row",
    backgroundColor: "#FAFAFA",
    borderRadius: 20,
    padding: 20,
    borderWidth: 1,
    borderColor: "#F0F0F0",
    justifyContent: "space-around",
  },
  statItem: { alignItems: "center", gap: 6 },
  statInfo: { alignItems: "center" },
  statValue: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  statLabel: {
    fontSize: 11,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  statDivider: {
    width: 1,
    backgroundColor: "#E0E0E0",
  },
  itemsSection: {},
  filterRow: {
    flexDirection: "row",
    gap: 8,
    marginBottom: 12,
  },
  filterChip: {
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 100,
    borderWidth: 1,
    borderColor: "#E0E0E0",
  },
  filterChipActive: {
    borderColor: COLORS.primary,
    backgroundColor: COLORS.primary,
  },
  filterChipText: {
    fontSize: 13,
    fontFamily: "Outfit_500Medium",
    color: "#666",
  },
  filterChipTextActive: { color: "#fff" },
  itemRow: {
    flexDirection: "row",
    alignItems: "center",
    padding: 14,
    backgroundColor: "#FAFAFA",
    borderRadius: 16,
    marginBottom: 8,
  },
  itemInfo: { flex: 1, marginRight: 12 },
  itemName: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
    marginBottom: 2,
  },
  itemAddress: {
    fontSize: 11,
    fontFamily: "Outfit_400Regular",
    color: "#999",
  },
  itemRight: { alignItems: "flex-end", gap: 4 },
  itemAmount: {
    fontSize: 14,
    fontFamily: "Outfit_700Bold",
    color: COLORS.black,
  },
  statusBadge: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 8,
    paddingVertical: 3,
    borderRadius: 100,
    gap: 4,
  },
  statusText: { fontSize: 11, fontFamily: "Outfit_600SemiBold" },
  exportBtn: {
    width: 40,
    height: 40,
    borderRadius: 20,
    justifyContent: "center",
    alignItems: "center",
  },
  // Off-screen but rendered (and collapsable=false) so captureRef can snapshot it.
  hiddenReceipt: {
    position: "absolute",
    left: -9999,
    top: 0,
    width: 340,
    backgroundColor: COLORS.white,
    padding: 24,
  },
  receiptCard: {
    backgroundColor: COLORS.white,
    borderRadius: 24,
    padding: 24,
    borderWidth: 1,
    borderColor: "#E0E0E0",
  },
  receiptHeader: {
    alignItems: "center",
    marginBottom: 8,
  },
  receiptBrand: {
    fontSize: 30,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  receiptTitle: {
    fontSize: 13,
    fontFamily: "Outfit_500Medium",
    color: "#999",
    marginTop: 2,
  },
  receiptDivider: {
    height: 1,
    backgroundColor: "#E8E8E8",
    marginVertical: 14,
  },
  receiptRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    paddingVertical: 5,
  },
  receiptLabel: {
    fontSize: 13,
    fontFamily: "Outfit_400Regular",
    color: "#666",
  },
  receiptValue: {
    fontSize: 13,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
  },
  receiptTotalRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "baseline",
  },
  receiptTotalLabel: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.black,
  },
  receiptTotalValue: {
    fontSize: 20,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  receiptFooter: {
    fontSize: 11,
    fontFamily: "Outfit_400Regular",
    color: "#bbb",
    textAlign: "center",
  },
});
