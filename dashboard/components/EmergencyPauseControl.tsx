"use client";

import React, { useState, useEffect, useCallback, useId } from "react";
import { AlertOctagon, ShieldAlert, CheckCircle2, X } from "lucide-react";

export interface EmergencyPauseControlProps {
  paused: boolean;
  onTogglePause: (nextPaused: boolean) => void | Promise<void>;
  isSuperAdmin?: boolean;
  disabled?: boolean;
}

/**
 * EmergencyPauseControl (#1013)
 * Prominent emergency freeze button controls with double-validation confirmation drawer.
 * Built with accessible ARIA semantics and motion-safe transitions to prevent motion sickness.
 */
export default function EmergencyPauseControl({
  paused,
  onTogglePause,
  isSuperAdmin = true,
  disabled = false,
}: EmergencyPauseControlProps) {
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [ackChecked, setAckChecked] = useState(false);
  const [typedConfirm, setTypedConfirm] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const titleId = useId();
  const descId = useId();

  const targetAction = paused ? "resume" : "pause";
  const requiredPhrase = paused ? "RESUME" : "FREEZE";
  const isDoubleVerified = ackChecked && typedConfirm.trim().toUpperCase() === requiredPhrase;

  const handleOpenDrawer = () => {
    if (!isSuperAdmin || disabled) return;
    setAckChecked(false);
    setTypedConfirm("");
    setDrawerOpen(true);
  };

  const handleCloseDrawer = useCallback(() => {
    setDrawerOpen(false);
    setAckChecked(false);
    setTypedConfirm("");
  }, []);

  // Keyboard accessibility: Escape closes the confirmation drawer
  useEffect(() => {
    if (!drawerOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        handleCloseDrawer();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [drawerOpen, handleCloseDrawer]);

  const handleExecute = async () => {
    if (!isDoubleVerified || submitting) return;
    setSubmitting(true);
    try {
      await onTogglePause(!paused);
      handleCloseDrawer();
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="space-y-3" data-testid="emergency-pause-section">
      {/* Prominent Emergency Button Banner with Crimson highlights */}
      <div
        className={`flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 p-4 rounded-xl border ${
          paused
            ? "border-red-500 bg-red-50/80 dark:bg-red-950/40 dark:border-red-800"
            : "border-rose-200 bg-rose-50/50 dark:bg-rose-950/20 dark:border-rose-900/60"
        } motion-safe:transition-colors motion-reduce:transition-none`}
      >
        <div className="flex items-start gap-3">
          <div
            className={`p-2 rounded-lg shrink-0 ${
              paused
                ? "bg-red-600 text-white"
                : "bg-rose-100 text-rose-700 dark:bg-rose-900/60 dark:text-rose-300"
            }`}
          >
            <ShieldAlert size={20} aria-hidden="true" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h3 className="text-sm font-bold text-slate-900 dark:text-slate-100">
                Vault Operations Status
              </h3>
              <span
                data-testid="vault-status-indicator"
                className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-semibold ${
                  paused
                    ? "bg-red-600 text-white animate-pulse"
                    : "bg-emerald-100 text-emerald-800 dark:bg-emerald-950/60 dark:text-emerald-300"
                }`}
              >
                {paused ? "EMERGENCY FROZEN" : "ACTIVE / OPERATIONAL"}
              </span>
            </div>
            <p className="text-xs text-slate-600 dark:text-slate-400 mt-1 max-w-md">
              {paused
                ? "All deposit, withdraw, and yield accrual functions are currently paused."
                : "Emergency kill switch immediately halts all deposits, withdrawals, and contract execution."}
            </p>
          </div>
        </div>

        {/* Action Trigger Button */}
        <button
          type="button"
          onClick={handleOpenDrawer}
          disabled={!isSuperAdmin || disabled}
          aria-disabled={!isSuperAdmin || disabled}
          aria-haspopup="dialog"
          aria-expanded={drawerOpen}
          data-testid="emergency-pause-toggle-btn"
          className={`px-4 py-2 text-xs font-semibold rounded-lg shadow-sm focus:outline-none focus:ring-2 focus:ring-offset-2 transition-all duration-200 motion-reduce:transition-none ${
            paused
              ? "bg-emerald-600 hover:bg-emerald-700 text-white focus:ring-emerald-500"
              : "bg-red-600 hover:bg-red-700 text-white focus:ring-red-500 hover:shadow-red-200 dark:hover:shadow-none"
          } disabled:opacity-50 disabled:cursor-not-allowed`}
        >
          {paused ? "Resume Vault Operations" : "Emergency Freeze Vault"}
        </button>
      </div>

      {/* ── Confirmation Drawer / Modal with double validation verification ── */}
      {drawerOpen && (
        <div
          role="dialog"
          aria-modal="true"
          aria-labelledby={titleId}
          aria-describedby={descId}
          data-testid="pause-confirmation-drawer"
          className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-900/60 backdrop-blur-xs motion-safe:transition-opacity motion-reduce:transition-none"
        >
          <div
            className="w-full max-w-lg bg-white dark:bg-slate-900 border border-red-200 dark:border-red-900 rounded-2xl shadow-2xl p-6 motion-safe:transition-all motion-safe:duration-200 motion-reduce:transition-none motion-reduce:transform-none"
          >
            {/* Header */}
            <div className="flex items-start justify-between pb-3 border-b border-slate-100 dark:border-slate-800">
              <div className="flex items-center gap-2 text-red-600 dark:text-red-400">
                <AlertOctagon size={24} aria-hidden="true" />
                <h2 id={titleId} className="text-base font-bold text-slate-900 dark:text-white">
                  {paused ? "Confirm Vault Operations Resumption" : "Emergency Vault Freeze Verification"}
                </h2>
              </div>
              <button
                type="button"
                onClick={handleCloseDrawer}
                aria-label="Close dialog"
                className="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 rounded-lg p-1"
              >
                <X size={18} />
              </button>
            </div>

            {/* Description */}
            <div className="mt-4 space-y-4 text-xs text-slate-600 dark:text-slate-300">
              <div
                id={descId}
                className="p-3 rounded-lg bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-900/60 text-red-800 dark:text-red-200"
              >
                <p className="font-semibold mb-1">
                  {paused
                    ? "Warning: Resuming vault operations"
                    : "CRITICAL: Double Validation Required"}
                </p>
                <p>
                  {paused
                    ? "Resuming will immediately re-enable Soroban contract deposit and withdrawal functions."
                    : "Freezing the vault immediately halts user withdrawals, deposits, and on-chain yield accrual."}
                </p>
              </div>

              {/* Validation Step 1: Explicit Checkbox */}
              <label className="flex items-start gap-2.5 cursor-pointer select-none">
                <input
                  type="checkbox"
                  checked={ackChecked}
                  onChange={(e) => setAckChecked(e.target.checked)}
                  data-testid="drawer-ack-checkbox"
                  className="mt-0.5 rounded border-slate-300 text-red-600 focus:ring-red-500"
                />
                <span className="text-slate-700 dark:text-slate-300">
                  I acknowledge that I am initiating an administrative state change on the Soroban smart contract.
                </span>
              </label>

              {/* Validation Step 2: Verification Phrase */}
              <div className="space-y-1.5">
                <label className="block font-medium text-slate-700 dark:text-slate-200">
                  Type <span className="font-mono font-bold text-red-600 dark:text-red-400">{requiredPhrase}</span> to confirm:
                </label>
                <input
                  type="text"
                  value={typedConfirm}
                  onChange={(e) => setTypedConfirm(e.target.value)}
                  placeholder={requiredPhrase}
                  data-testid="drawer-confirm-input"
                  className="w-full px-3 py-2 text-xs font-mono border rounded-lg border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-red-500"
                />
              </div>
            </div>

            {/* Footer Buttons */}
            <div className="mt-6 flex items-center justify-end gap-3 pt-3 border-t border-slate-100 dark:border-slate-800">
              <button
                type="button"
                onClick={handleCloseDrawer}
                className="px-3.5 py-2 text-xs font-medium text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-white rounded-lg transition-colors"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleExecute}
                disabled={!isDoubleVerified || submitting}
                data-testid="drawer-final-confirm-btn"
                className={`px-4 py-2 text-xs font-semibold text-white rounded-lg transition-colors ${
                  paused
                    ? "bg-emerald-600 hover:bg-emerald-700"
                    : "bg-red-600 hover:bg-red-700"
                } disabled:opacity-40 disabled:cursor-not-allowed`}
              >
                {submitting
                  ? "Broadcasting Transaction…"
                  : paused
                  ? "Confirm Resume Vault"
                  : "Execute Emergency Freeze"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
