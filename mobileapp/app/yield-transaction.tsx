import React, { useState, useRef, useCallback, useMemo, useEffect } from "react";
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  Modal,
  KeyboardAvoidingView,
  Platform,
  PanResponder,
  GestureResponderEvent,
  PanResponderGestureState,
  ActivityIndicator,
  TextInput,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { useRouter, Stack } from "expo-router";
import { COLORS } from "../src/constants/colors";
import { useBiometric } from "../hooks/useBiometric";
import { fetchYieldBalance, apiFetch } from "../src/services/api";

// ── Types ────────────────────────────────────────────────────────────────────

export type TransactionType = "deposit" | "withdraw";

export interface YieldTransactionProps {
  /** Current APY rate as a percentage (e.g. 5.00 for 5%) */
  apy?: number;
  /** Initial mode ('deposit' | 'withdraw') */
  initialMode?: TransactionType;
  /** Available user balance */
  initialAvailableBalance?: number;
  /** Current earning / yield balance */
  initialYieldBalance?: number;
  /** Called when the user confirms the transaction */
  onConfirm?: (amount: number, type: TransactionType) => Promise<void>;
  /** Called when the user cancels */
  onCancel?: () => void;
  /** Optional modal drawer visibility for embedded usage */
  visible?: boolean;
}

const NAIRA_KEYPAD_KEYS = [
  "1",
  "2",
  "3",
  "4",
  "5",
  "6",
  "7",
  "8",
  "9",
  ".",
  "0",
  "⌫",
];

// ── Percentage Slider ─────────────────────────────────────────────────────────

const SNAP_PERCENTAGES = [0, 25, 50, 75, 100];

interface PercentageSliderProps {
  /** Current selected percentage (0–100). */
  value: number;
  /** Called with the snapped percentage when the user slides or taps a tick. */
  onChange: (pct: number) => void;
}

/**
 * Horizontal slider that snaps to 0 / 25 / 50 / 75 / 100 %.
 * Dragging the thumb or tapping a tick label both update the selection.
 */
function PercentageSlider({ value, onChange }: PercentageSliderProps) {
  const trackWidth = useRef(0);

  const snapToNearest = useCallback(
    (rawPct: number) => {
      const clamped = Math.max(0, Math.min(100, rawPct));
      let closest = SNAP_PERCENTAGES[0];
      let minDist = Math.abs(clamped - closest);
      for (const snap of SNAP_PERCENTAGES) {
        const dist = Math.abs(clamped - snap);
        if (dist < minDist) {
          minDist = dist;
          closest = snap;
        }
      }
      onChange(closest);
    },
    [onChange]
  );

  const panResponder = useRef(
    PanResponder.create({
      onStartShouldSetPanResponder: () => true,
      onMoveShouldSetPanResponder: () => true,
      onPanResponderGrant: (
        evt: GestureResponderEvent,
        _state: PanResponderGestureState
      ) => {
        if (trackWidth.current === 0) return;
        const x = evt.nativeEvent.locationX;
        const rawPct = (x / trackWidth.current) * 100;
        snapToNearest(rawPct);
      },
      onPanResponderMove: (
        evt: GestureResponderEvent,
        _state: PanResponderGestureState
      ) => {
        if (trackWidth.current === 0) return;
        const x = evt.nativeEvent.locationX;
        const rawPct = (x / trackWidth.current) * 100;
        snapToNearest(rawPct);
      },
    })
  ).current;

  const thumbPosition = `${value}%` as `${number}%`;

  return (
    <View style={sliderStyles.wrapper}>
      {/* Track */}
      <View
        style={sliderStyles.trackContainer}
        onLayout={(e) => {
          trackWidth.current = e.nativeEvent.layout.width;
        }}
        {...panResponder.panHandlers}
        accessible={false}
      >
        {/* Filled portion */}
        <View style={[sliderStyles.trackFill, { width: thumbPosition }]} />
        {/* Tick marks */}
        {SNAP_PERCENTAGES.map((pct) => (
          <View
            key={pct}
            style={[
              sliderStyles.tick,
              { left: `${pct}%` as `${number}%` },
              pct === value && sliderStyles.tickActive,
            ]}
          />
        ))}
        {/* Thumb */}
        <View
          style={[sliderStyles.thumb, { left: thumbPosition }]}
          accessibilityRole="adjustable"
          accessibilityValue={{ min: 0, max: 100, now: value }}
          accessibilityLabel={`Percentage slider, ${value}%`}
        />
      </View>

      {/* Snap-point labels */}
      <View style={sliderStyles.labelsRow}>
        {SNAP_PERCENTAGES.filter((p) => p > 0).map((pct) => (
          <TouchableOpacity
            key={pct}
            style={[
              sliderStyles.labelBtn,
              pct === value && sliderStyles.labelBtnActive,
            ]}
            onPress={() => onChange(pct)}
            activeOpacity={0.75}
            accessibilityRole="button"
            accessibilityLabel={`Set ${pct} percent`}
          >
            <Text
              style={[
                sliderStyles.labelText,
                pct === value && sliderStyles.labelTextActive,
              ]}
            >
              {pct}%
            </Text>
          </TouchableOpacity>
        ))}
      </View>
    </View>
  );
}

// ── Naira Keypad ─────────────────────────────────────────────────────────────

function NairaKeypad({ onPress }: { onPress: (key: string) => void }) {
  return (
    <View style={styles.keypad}>
      {NAIRA_KEYPAD_KEYS.map((key) => (
        <TouchableOpacity
          key={key}
          style={styles.keypadKey}
          onPress={() => onPress(key)}
          activeOpacity={0.7}
          accessibilityRole="button"
          accessibilityLabel={`Keypad ${key === "⌫" ? "Backspace" : key}`}
        >
          <Text style={styles.keypadKeyText}>{key}</Text>
        </TouchableOpacity>
      ))}
    </View>
  );
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function formatCurrency(value: number): string {
  return `₦${value.toLocaleString("en-NG", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

// ── Main Screen & Component ──────────────────────────────────────────────────

export default function YieldTransaction(props: YieldTransactionProps = {}) {
  const router = useRouter();
  const biometric = useBiometric();

  const [mode, setMode] = useState<TransactionType>(
    props.initialMode || "deposit"
  );
  const [amount, setAmount] = useState("");
  const [showConfirm, setShowConfirm] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [sliderPct, setSliderPct] = useState(0);

  const [availableBalance, setAvailableBalance] = useState(
    props.initialAvailableBalance ?? 150_000
  );
  const [yieldBalance, setYieldBalance] = useState(
    props.initialYieldBalance ?? 23_480
  );
  const [apy, setApy] = useState<number>(props.apy ?? 5.0);

  useEffect(() => {
    let isMounted = true;
    (async () => {
      try {
        const data = await fetchYieldBalance();
        if (!isMounted) return;
        if (data.apy) {
          const parsedApy =
            typeof data.apy === "number"
              ? data.apy
              : parseFloat(String(data.apy).replace(/[^0-9.]/g, "")) || 5.0;
          setApy(parsedApy);
        }
        if (data.availableBalance !== undefined) {
          const avail =
            typeof data.availableBalance === "number"
              ? data.availableBalance
              : parseFloat(String(data.availableBalance).replace(/[^0-9.]/g, "")) || 150_000;
          setAvailableBalance(avail);
        }
        if (data.totalYieldEarned !== undefined || data.earningBalance !== undefined) {
          const earnVal = data.earningBalance ?? data.totalYieldEarned;
          const earn =
            typeof earnVal === "number"
              ? earnVal
              : parseFloat(String(earnVal).replace(/[^0-9.]/g, "")) || 23_480;
          setYieldBalance(earn);
        }
      } catch {
        // Fallback to initial balances
      }
    })();
    return () => {
      isMounted = false;
    };
  }, []);

  const activeBalance = mode === "deposit" ? availableBalance : yieldBalance;
  const balanceLabel =
    mode === "deposit" ? "Available Balance" : "Earning Balance";

  const numericAmount = parseFloat(amount) || 0;
  const exceedsLimit = numericAmount > activeBalance;
  const isValid = numericAmount > 0 && !exceedsLimit;

  // Projected returns (Deposit mode only)
  const projectedAnnualReturn = useMemo(
    () => (numericAmount * apy) / 100,
    [numericAmount, apy]
  );
  const projectedMonthlyReturn = useMemo(
    () => projectedAnnualReturn / 12,
    [projectedAnnualReturn]
  );

  function handleKeyPress(key: string) {
    setSliderPct(0);
    if (key === "⌫") {
      setAmount((prev) => prev.slice(0, -1));
      return;
    }
    if (key === "." && amount.includes(".")) return;
    if (amount === "0" && key !== ".") {
      setAmount(key);
      return;
    }
    // Cap at 2 decimal places
    const parts = (amount + key).split(".");
    if (parts[1] && parts[1].length > 2) return;
    setAmount((prev) => prev + key);
  }

  function handleSliderChange(pct: number) {
    setSliderPct(pct);
    if (pct === 0) {
      setAmount("");
    } else {
      setAmount(((activeBalance * pct) / 100).toFixed(2));
    }
  }

  const handleConfirmAction = async () => {
    if (!isValid || isProcessing) return;

    if (biometric.enabled) {
      const success = await biometric.authenticate("paymentConfirm");
      if (!success) return;
    }

    setIsProcessing(true);
    try {
      if (props.onConfirm) {
        await props.onConfirm(numericAmount, mode);
      } else {
        // Post yield deposit or withdrawal to backend
        await apiFetch(`/api/yield/${mode}`, {
          method: "POST",
          body: JSON.stringify({ amount: numericAmount }),
        }).catch(() => {});
      }

      // Update local balances
      if (mode === "deposit") {
        setAvailableBalance((prev) => Math.max(0, prev - numericAmount));
        setYieldBalance((prev) => prev + numericAmount);
      } else {
        setYieldBalance((prev) => Math.max(0, prev - numericAmount));
        setAvailableBalance((prev) => prev + numericAmount);
      }

      setShowConfirm(false);
      setSubmitted(true);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleClose = () => {
    if (props.onCancel) {
      props.onCancel();
    } else {
      router.back();
    }
  };

  // ── Success View ───────────────────────────────────────────────────────────
  const renderSuccessView = () => (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />
      <View style={styles.successContainer}>
        <View style={styles.successCircle}>
          <Ionicons name="checkmark" size={56} color={COLORS.primary} />
        </View>
        <Text style={styles.successTitle}>
          {mode === "deposit"
            ? "Deposit Successful!"
            : "Withdrawal Successful!"}
        </Text>
        <Text style={styles.successSubtitle}>
          {formatCurrency(numericAmount)}{" "}
          {mode === "deposit"
            ? "has been moved to your earning balance."
            : "has been returned to your available balance."}
        </Text>
        <TouchableOpacity
          style={styles.doneButton}
          onPress={() => {
            if (props.onCancel) {
              props.onCancel();
            } else {
              router.replace("/(personal)/home");
            }
          }}
          activeOpacity={0.8}
        >
          <Text style={styles.doneButtonText}>Back to Home</Text>
        </TouchableOpacity>
      </View>
    </SafeAreaView>
  );

  // ── Main Content View ──────────────────────────────────────────────────────
  const renderMainContent = () => (
    <SafeAreaView style={styles.container}>
      <Stack.Screen options={{ headerShown: false }} />

      {/* Header */}
      <View style={styles.header}>
        <TouchableOpacity
          onPress={handleClose}
          style={styles.backButton}
          accessibilityLabel="Back"
        >
          <Ionicons name="arrow-back" size={24} color={COLORS.primary} />
        </TouchableOpacity>
        <Text style={styles.headerTitle}>Yield Vault Transaction</Text>
        <View style={{ width: 40 }} />
      </View>

      <KeyboardAvoidingView
        style={{ flex: 1 }}
        behavior={Platform.OS === "ios" ? "padding" : undefined}
      >
        <ScrollView
          contentContainerStyle={styles.scrollContent}
          keyboardShouldPersistTaps="handled"
          showsVerticalScrollIndicator={false}
        >
          {/* Mode toggle */}
          <View style={styles.toggleRow}>
            <TouchableOpacity
              style={[
                styles.toggleBtn,
                mode === "deposit" && styles.toggleBtnActive,
              ]}
              onPress={() => {
                setMode("deposit");
                setAmount("");
                setSliderPct(0);
              }}
              activeOpacity={0.8}
            >
              <Ionicons
                name="arrow-down-circle-outline"
                size={18}
                color={mode === "deposit" ? COLORS.secondary : COLORS.primary}
              />
              <Text
                style={[
                  styles.toggleBtnText,
                  mode === "deposit" && styles.toggleBtnTextActive,
                ]}
              >
                Deposit
              </Text>
            </TouchableOpacity>

            <TouchableOpacity
              style={[
                styles.toggleBtn,
                mode === "withdraw" && styles.toggleBtnActive,
              ]}
              onPress={() => {
                setMode("withdraw");
                setAmount("");
                setSliderPct(0);
              }}
              activeOpacity={0.8}
            >
              <Ionicons
                name="arrow-up-circle-outline"
                size={18}
                color={mode === "withdraw" ? COLORS.secondary : COLORS.primary}
              />
              <Text
                style={[
                  styles.toggleBtnText,
                  mode === "withdraw" && styles.toggleBtnTextActive,
                ]}
              >
                Withdraw
              </Text>
            </TouchableOpacity>
          </View>

          {/* Balance display */}
          <View style={styles.balanceCard}>
            <Text style={styles.balanceLabel}>{balanceLabel}</Text>
            <Text style={styles.balanceAmount}>
              ₦{activeBalance.toLocaleString("en-NG", { minimumFractionDigits: 2 })}
            </Text>
          </View>

          {/* APY Banner (Deposit mode) */}
          {mode === "deposit" && (
            <View style={styles.apyBadge}>
              <Text style={styles.apyBadgeText}>Current Vault APY</Text>
              <Text style={styles.apyValue}>{apy.toFixed(2)}%</Text>
            </View>
          )}

          {/* Flow description */}
          <Text style={styles.description}>
            {mode === "deposit"
              ? "Move funds from your available balance into your yield-earning vault to start earning interest."
              : "Move funds from your yield-earning vault back to your available balance for instant spending."}
          </Text>

          {/* Amount input & limits check display */}
          <View
            style={[
              styles.amountDisplay,
              exceedsLimit && styles.amountDisplayError,
            ]}
          >
            <Text style={styles.amountCurrency}>₦</Text>
            <Text
              style={[styles.amountValue, !amount && styles.amountPlaceholder]}
            >
              {amount || "0.00"}
            </Text>
          </View>

          {exceedsLimit && (
            <Text style={styles.limitError}>
              Amount exceeds your {balanceLabel.toLowerCase()} of ₦
              {activeBalance.toLocaleString("en-NG", { minimumFractionDigits: 2 })}
            </Text>
          )}

          {/* Projected Returns (Deposit mode) */}
          {mode === "deposit" && numericAmount > 0 && !exceedsLimit && (
            <View style={styles.projectionCard}>
              <Text style={styles.projectionTitle}>Projected Returns</Text>
              <View style={styles.projectionRow}>
                <Text style={styles.projectionLabel}>Monthly</Text>
                <Text style={styles.projectionValue}>
                  {formatCurrency(projectedMonthlyReturn)}
                </Text>
              </View>
              <View style={styles.divider} />
              <View style={styles.projectionRow}>
                <Text style={styles.projectionLabel}>Annually</Text>
                <Text style={[styles.projectionValue, styles.projectionHighlight]}>
                  {formatCurrency(projectedAnnualReturn)}
                </Text>
              </View>
            </View>
          )}

          {/* Percentage slider */}
          <PercentageSlider
            value={sliderPct}
            onChange={handleSliderChange}
          />

          {/* Custom Naira keypad */}
          <NairaKeypad onPress={handleKeyPress} />

          {/* Confirmation trigger button */}
          <TouchableOpacity
            style={[
              styles.confirmButton,
              !isValid && styles.confirmButtonDisabled,
            ]}
            onPress={() => isValid && setShowConfirm(true)}
            disabled={!isValid}
            activeOpacity={0.8}
          >
            <Text style={styles.confirmButtonText}>
              {mode === "deposit" ? "Deposit to Yield" : "Withdraw from Yield"}
            </Text>
          </TouchableOpacity>
        </ScrollView>
      </KeyboardAvoidingView>

      {/* Confirmation modal */}
      <Modal visible={showConfirm} transparent animationType="slide">
        <View style={styles.modalOverlay}>
          <View style={styles.modalCard}>
            <Text style={styles.modalTitle}>
              Confirm {mode === "deposit" ? "Deposit" : "Withdrawal"}
            </Text>

            <View style={styles.modalRow}>
              <Text style={styles.modalLabel}>Amount</Text>
              <Text style={styles.modalValue}>{formatCurrency(numericAmount)}</Text>
            </View>
            <View style={styles.modalRow}>
              <Text style={styles.modalLabel}>From</Text>
              <Text style={styles.modalValue}>
                {mode === "deposit" ? "Available Balance" : "Earning Balance"}
              </Text>
            </View>
            <View style={styles.modalRow}>
              <Text style={styles.modalLabel}>To</Text>
              <Text style={styles.modalValue}>
                {mode === "deposit" ? "Earning Balance" : "Available Balance"}
              </Text>
            </View>

            {mode === "deposit" && (
              <View style={styles.modalRow}>
                <Text style={styles.modalLabel}>Annual Rate</Text>
                <Text style={styles.modalValue}>{apy.toFixed(2)}% APY</Text>
              </View>
            )}

            {isProcessing ? (
              <ActivityIndicator
                size="large"
                color={COLORS.primary}
                style={{ marginVertical: 20 }}
              />
            ) : (
              <View style={styles.modalActions}>
                <TouchableOpacity
                  style={[styles.modalBtn, styles.modalBtnCancel]}
                  onPress={() => setShowConfirm(false)}
                >
                  <Text style={styles.modalBtnCancelText}>Cancel</Text>
                </TouchableOpacity>
                <TouchableOpacity
                  style={[styles.modalBtn, styles.modalBtnConfirm]}
                  onPress={handleConfirmAction}
                >
                  <Text style={styles.modalBtnConfirmText}>
                    {biometric.enabled ? "Authenticate & Confirm" : "Confirm"}
                  </Text>
                </TouchableOpacity>
              </View>
            )}
          </View>
        </View>
      </Modal>
    </SafeAreaView>
  );

  // If used as an embedded drawer modal with `visible` prop
  if (props.visible !== undefined) {
    return (
      <Modal visible={props.visible} transparent animationType="slide">
        <View style={styles.modalOverlay}>
          <View style={[styles.modalCard, { maxHeight: "90%" }]}>
            {submitted ? renderSuccessView() : renderMainContent()}
          </View>
        </View>
      </Modal>
    );
  }

  if (submitted) {
    return renderSuccessView();
  }

  return renderMainContent();
}

// ── Styles ───────────────────────────────────────────────────────────────────

const sliderStyles = StyleSheet.create({
  wrapper: {
    marginBottom: 20,
  },
  trackContainer: {
    height: 28,
    backgroundColor: "#E8E8E8",
    borderRadius: 14,
    marginHorizontal: 8,
    justifyContent: "center",
    overflow: "visible",
    position: "relative",
  },
  trackFill: {
    position: "absolute",
    left: 0,
    top: 0,
    bottom: 0,
    backgroundColor: COLORS.primary,
    borderRadius: 14,
  },
  tick: {
    position: "absolute",
    width: 2,
    height: 10,
    backgroundColor: "#CCCCCC",
    borderRadius: 1,
    top: 9,
    marginLeft: -1,
  },
  tickActive: {
    backgroundColor: COLORS.secondary,
  },
  thumb: {
    position: "absolute",
    width: 28,
    height: 28,
    borderRadius: 14,
    backgroundColor: COLORS.white,
    borderWidth: 2.5,
    borderColor: COLORS.primary,
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.12,
    shadowRadius: 4,
    elevation: 3,
    marginLeft: -14,
    top: 0,
  },
  labelsRow: {
    flexDirection: "row",
    justifyContent: "space-around",
    marginTop: 10,
    marginHorizontal: 8,
  },
  labelBtn: {
    paddingVertical: 6,
    paddingHorizontal: 12,
    borderRadius: 8,
    backgroundColor: "#F0F0F0",
    minWidth: 52,
    alignItems: "center",
  },
  labelBtnActive: {
    backgroundColor: COLORS.primary,
  },
  labelText: {
    fontSize: 13,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
  labelTextActive: {
    color: COLORS.secondary,
  },
});

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: COLORS.white,
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 20,
    paddingVertical: 14,
  },
  backButton: {
    width: 40,
    height: 40,
    borderRadius: 20,
    justifyContent: "center",
    alignItems: "center",
  },
  headerTitle: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  scrollContent: {
    paddingHorizontal: 20,
    paddingBottom: 40,
  },
  toggleRow: {
    flexDirection: "row",
    gap: 12,
    marginBottom: 16,
    marginTop: 4,
  },
  toggleBtn: {
    flex: 1,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 6,
    paddingVertical: 12,
    borderRadius: 12,
    borderWidth: 1.5,
    borderColor: COLORS.primary,
    backgroundColor: COLORS.white,
  },
  toggleBtnActive: {
    backgroundColor: COLORS.primary,
    borderColor: COLORS.primary,
  },
  toggleBtnText: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
  toggleBtnTextActive: {
    color: COLORS.secondary,
  },
  balanceCard: {
    backgroundColor: "#F0FAF0",
    borderRadius: 14,
    padding: 16,
    marginBottom: 14,
    borderWidth: 1,
    borderColor: "#D0EDD0",
  },
  balanceLabel: {
    fontSize: 12,
    color: "#666",
    fontFamily: "Outfit_400Regular",
    marginBottom: 4,
  },
  balanceAmount: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  apyBadge: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    backgroundColor: "#f0fdf4",
    borderRadius: 12,
    padding: 14,
    marginBottom: 14,
    borderWidth: 1,
    borderColor: "#bbf7d0",
  },
  apyBadgeText: {
    fontSize: 14,
    color: "#166534",
    fontFamily: "Outfit_500Medium",
  },
  apyValue: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: "#16a34a",
  },
  description: {
    fontSize: 13,
    color: "#777",
    fontFamily: "Outfit_400Regular",
    lineHeight: 20,
    marginBottom: 16,
  },
  amountDisplay: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    borderWidth: 2,
    borderColor: COLORS.primary,
    borderRadius: 16,
    paddingVertical: 18,
    marginBottom: 6,
    backgroundColor: "#FAFFFE",
  },
  amountDisplayError: {
    borderColor: "#CC0000",
    backgroundColor: "#FFF5F5",
  },
  amountCurrency: {
    fontSize: 28,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
    marginRight: 4,
  },
  amountValue: {
    fontSize: 40,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
  },
  amountPlaceholder: {
    color: "#CCCCCC",
  },
  limitError: {
    fontSize: 12,
    color: "#CC0000",
    fontFamily: "Outfit_500Medium",
    textAlign: "center",
    marginBottom: 12,
  },
  projectionCard: {
    backgroundColor: "#f9fafb",
    borderRadius: 12,
    padding: 16,
    marginBottom: 16,
    borderWidth: 1,
    borderColor: "#e5e7eb",
  },
  projectionTitle: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: "#6b7280",
    marginBottom: 10,
  },
  projectionRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    paddingVertical: 4,
  },
  projectionLabel: {
    fontSize: 14,
    color: "#374151",
    fontFamily: "Outfit_400Regular",
  },
  projectionValue: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: "#111827",
  },
  projectionHighlight: {
    fontSize: 16,
    color: "#16a34a",
    fontFamily: "Outfit_700Bold",
  },
  divider: {
    height: 1,
    backgroundColor: "#e5e7eb",
    marginVertical: 8,
  },
  keypad: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 10,
    marginBottom: 20,
    justifyContent: "center",
  },
  keypadKey: {
    width: "30%",
    aspectRatio: 2.2,
    justifyContent: "center",
    alignItems: "center",
    backgroundColor: "#F5F5F5",
    borderRadius: 12,
  },
  keypadKeyText: {
    fontSize: 22,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
  confirmButton: {
    backgroundColor: COLORS.primary,
    borderRadius: 28,
    height: 56,
    justifyContent: "center",
    alignItems: "center",
  },
  confirmButtonDisabled: {
    backgroundColor: "#CCCCCC",
  },
  confirmButtonText: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: COLORS.secondary,
  },
  modalOverlay: {
    flex: 1,
    backgroundColor: "rgba(0,0,0,0.4)",
    justifyContent: "flex-end",
  },
  modalCard: {
    backgroundColor: COLORS.white,
    borderTopLeftRadius: 24,
    borderTopRightRadius: 24,
    padding: 24,
    paddingBottom: 36,
  },
  modalTitle: {
    fontSize: 18,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
    marginBottom: 20,
    textAlign: "center",
  },
  modalRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    marginBottom: 12,
  },
  modalLabel: {
    fontSize: 14,
    color: "#666",
    fontFamily: "Outfit_400Regular",
  },
  modalValue: {
    fontSize: 14,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
  modalActions: {
    flexDirection: "row",
    gap: 12,
    marginTop: 24,
  },
  modalBtn: {
    flex: 1,
    height: 52,
    borderRadius: 26,
    justifyContent: "center",
    alignItems: "center",
  },
  modalBtnCancel: {
    borderWidth: 1.5,
    borderColor: COLORS.primary,
  },
  modalBtnConfirm: {
    backgroundColor: COLORS.primary,
  },
  modalBtnCancelText: {
    fontSize: 15,
    fontFamily: "Outfit_600SemiBold",
    color: COLORS.primary,
  },
  modalBtnConfirmText: {
    fontSize: 15,
    fontFamily: "Outfit_700Bold",
    color: COLORS.secondary,
  },
  successContainer: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
    paddingHorizontal: 32,
  },
  successCircle: {
    width: 100,
    height: 100,
    borderRadius: 50,
    borderWidth: 3,
    borderColor: COLORS.primary,
    justifyContent: "center",
    alignItems: "center",
    marginBottom: 24,
    backgroundColor: "#F0FAF0",
  },
  successTitle: {
    fontSize: 22,
    fontFamily: "Outfit_700Bold",
    color: COLORS.primary,
    marginBottom: 12,
    textAlign: "center",
  },
  successSubtitle: {
    fontSize: 14,
    color: "#666",
    fontFamily: "Outfit_400Regular",
    textAlign: "center",
    lineHeight: 22,
    marginBottom: 40,
  },
  doneButton: {
    backgroundColor: COLORS.primary,
    borderRadius: 28,
    height: 56,
    width: "100%",
    justifyContent: "center",
    alignItems: "center",
  },
  doneButtonText: {
    fontSize: 16,
    fontFamily: "Outfit_700Bold",
    color: COLORS.secondary,
  },
});
