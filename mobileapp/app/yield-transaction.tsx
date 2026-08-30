import React, { useState, useMemo } from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  Modal,
  StyleSheet,
  ActivityIndicator,
  TextInput,
} from 'react-native';
import { useBiometric } from '../hooks/useBiometric';

interface YieldTransactionProps {
  /** Current APY rate as a percentage (e.g. 5.00 for 5 %) */
  apy: number;
  /** Called when the user confirms the deposit */
  onConfirm: (amount: number) => Promise<void>;
  /** Called when the user cancels the drawer */
  onCancel: () => void;
  /** Controls drawer visibility */
  visible: boolean;
}

/**
 * Yield Vault – Deposit Confirmation Drawer
 *
 * Displays a bottom-sheet drawer that lets the user enter a deposit amount,
 * shows projected monthly and annual returns based on the current APY rate,
 * and asks for confirmation (with optional biometric authentication).
 *
 * @see https://github.com/Fracverse/zaps/issues/711
 */
export default function YieldTransaction({
  apy,
  onConfirm,
  onCancel,
  visible,
}: YieldTransactionProps) {
  const biometric = useBiometric();
  const [depositAmount, setDepositAmount] = useState('');
  const [isProcessing, setIsProcessing] = useState(false);

  const parsedAmount = useMemo(() => {
    const value = Number(depositAmount);
    return Number.isNaN(value) || value <= 0 ? 0 : value;
  }, [depositAmount]);

  // ── Projected returns ─────────────────────────────────────────────────
  // Return = (depositAmount * APY) / 100
  const projectedAnnualReturn = useMemo(
    () => (parsedAmount * apy) / 100,
    [parsedAmount, apy],
  );
  const projectedMonthlyReturn = useMemo(
    () => projectedAnnualReturn / 12,
    [projectedAnnualReturn],
  );

  const canConfirm = parsedAmount > 0 && !isProcessing;

  // ── Handlers ──────────────────────────────────────────────────────────
  const handleConfirm = async () => {
    if (!canConfirm) return;

    if (biometric.enabled) {
      const success = await biometric.authenticate('paymentConfirm');
      if (!success) return;
    }

    setIsProcessing(true);
    try {
      await onConfirm(parsedAmount);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleAmountChange = (text: string) => {
    // Allow only numeric input with optional decimal
    const cleaned = text.replace(/[^0-9.]/g, '');
    setDepositAmount(cleaned);
  };

  // ── Render ────────────────────────────────────────────────────────────
  return (
    <Modal visible={visible} transparent animationType="slide">
      <View style={styles.overlay}>
        <View style={styles.drawer}>
          <Text style={styles.title}>Deposit to Yield Vault</Text>

          {/* ── Amount input ─────────────────────────────────────── */}
          <Text style={styles.label}>Deposit Amount</Text>
          <TextInput
            style={styles.amountInput}
            value={depositAmount}
            onChangeText={handleAmountChange}
            keyboardType="decimal-pad"
            placeholder="0.00"
            placeholderTextColor="#9ca3af"
          />

          {/* ── APY banner ───────────────────────────────────────── */}
          <View style={styles.apyBadge}>
            <Text style={styles.apyBadgeText}>Current APY</Text>
            <Text style={styles.apyValue}>{apy.toFixed(2)}%</Text>
          </View>

          {/* ── Projected returns ────────────────────────────────── */}
          {parsedAmount > 0 && (
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

          {/* ── Actions ──────────────────────────────────────────── */}
          {isProcessing ? (
            <ActivityIndicator
              size="large"
              color="#6366f1"
              style={styles.spinner}
            />
          ) : (
            <View style={styles.buttonRow}>
              <TouchableOpacity style={styles.cancelButton} onPress={onCancel}>
                <Text style={styles.cancelText}>Cancel</Text>
              </TouchableOpacity>

              <TouchableOpacity
                style={[styles.confirmButton, !canConfirm && styles.confirmButtonDisabled]}
                onPress={handleConfirm}
                disabled={!canConfirm}
              >
                <Text style={styles.confirmText}>
                  {biometric.enabled ? 'Authenticate & Deposit' : 'Confirm Deposit'}
                </Text>
              </TouchableOpacity>
            </View>
          )}
        </View>
      </View>
    </Modal>
  );
}

// ── Helpers ──────────────────────────────────────────────────────────────

function formatCurrency(value: number): string {
  return `₦${value.toLocaleString('en-NG', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

// ── Styles ───────────────────────────────────────────────────────────────

const styles = StyleSheet.create({
  overlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.5)',
    justifyContent: 'flex-end',
  },
  drawer: {
    backgroundColor: '#fff',
    borderTopLeftRadius: 24,
    borderTopRightRadius: 24,
    padding: 24,
  },
  title: {
    fontSize: 20,
    fontWeight: '700',
    color: '#111827',
    marginBottom: 20,
  },
  label: {
    fontSize: 14,
    fontWeight: '600',
    color: '#374151',
    marginBottom: 8,
  },
  amountInput: {
    borderWidth: 1,
    borderColor: '#e5e7eb',
    borderRadius: 12,
    padding: 16,
    fontSize: 24,
    fontWeight: '600',
    color: '#111827',
    marginBottom: 16,
  },
  apyBadge: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    backgroundColor: '#f0fdf4',
    borderRadius: 12,
    padding: 14,
    marginBottom: 16,
  },
  apyBadgeText: {
    fontSize: 14,
    color: '#6b7280',
  },
  apyValue: {
    fontSize: 16,
    fontWeight: '700',
    color: '#16a34a',
  },
  projectionCard: {
    backgroundColor: '#f9fafb',
    borderRadius: 12,
    padding: 16,
    marginBottom: 20,
  },
  projectionTitle: {
    fontSize: 14,
    fontWeight: '600',
    color: '#6b7280',
    marginBottom: 12,
  },
  projectionRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: 6,
  },
  projectionLabel: {
    fontSize: 14,
    color: '#374151',
  },
  projectionValue: {
    fontSize: 14,
    fontWeight: '600',
    color: '#111827',
  },
  projectionHighlight: {
    fontSize: 16,
    color: '#16a34a',
    fontWeight: '700',
  },
  divider: {
    height: 1,
    backgroundColor: '#e5e7eb',
    marginVertical: 8,
  },
  buttonRow: {
    flexDirection: 'row',
    gap: 12,
  },
  cancelButton: {
    flex: 1,
    padding: 16,
    borderRadius: 12,
    backgroundColor: '#f3f4f6',
    alignItems: 'center',
  },
  cancelText: {
    color: '#374151',
    fontWeight: '600',
  },
  confirmButton: {
    flex: 2,
    padding: 16,
    borderRadius: 12,
    backgroundColor: '#6366f1',
    alignItems: 'center',
  },
  confirmButtonDisabled: {
    backgroundColor: '#a5b4fc',
  },
  confirmText: {
    color: '#fff',
    fontWeight: '600',
  },
  spinner: {
    marginVertical: 24,
  },
});
