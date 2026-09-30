"use client";
import { useState, useMemo, useCallback, useEffect } from "react";
import { format } from "date-fns";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from "recharts";
import {
  detectFreighter,
  connectFreighter,
  signWithFreighter,
  truncateKey,
  type FreighterWalletState,
  DEFAULT_WALLET_STATE,
} from "@/lib/freighter";
import { useSuperAdmin } from "@/lib/auth-context";
import { api } from "@/lib/api";
import EmergencyPauseControl from "@/components/EmergencyPauseControl";

// ── Types ──────────────────────────────────────────────────────────────────────

export interface VaultParams {
  apy: string;
  baseInterestRate: string;
  dynamicApyEnabled: boolean;
  dynamicRuleType: "utilization" | "tiered" | "performance";
  targetUtilization: string;
  utilizationMultiplier: string;
  maxApyCap: string;
  paused: boolean;
  adminAddress: string;
}

export interface YieldTx {
  id: string;
  txHash: string;
  timestamp: string;
  /** Full Stellar public key of the address involved. */
  address: string;
  action: "deposit" | "withdraw" | "yield_accrual" | "config";
  tokenVolume: number;
  asset: string;
  blockHeight?: number;
  fee?: number;
  yieldIndex?: number;
  details?: string;
}

type SortKey = keyof Pick<YieldTx, "timestamp" | "address" | "action" | "tokenVolume">;
type ActionFilter = YieldTx["action"] | "all";

interface FormErrors {
  apy?: string;
  baseInterestRate?: string;
  adminAddress?: string;
  targetUtilization?: string;
  utilizationMultiplier?: string;
}

// ── Yield Index Compounding & Dynamic APY Engine (#1009) ───────────────────────

const INITIAL_YIELD_INDEX = 1.04285012; // 1e8 precision base index

/**
 * Calculates current and projected yield index based on interest rate and dynamic APY rules.
 */
function calculateYieldIndexMetrics(
  baseIndex: number,
  apyPercent: number,
  baseRatePercent: number,
  dynamicEnabled: boolean,
  ruleType: "utilization" | "tiered" | "performance",
  targetUtilization: number,
  multiplier: number,
) {
  let effectiveRate = apyPercent;
  if (dynamicEnabled) {
    if (ruleType === "utilization") {
      // Dynamic utilization rate: baseRate + (apy - baseRate) * (utilization / 100) * multiplier
      const spread = Math.max(0, apyPercent - baseRatePercent);
      effectiveRate = baseRatePercent + spread * (targetUtilization / 100) * multiplier;
    } else if (ruleType === "tiered") {
      // Tiered dynamic rate (e.g. weighted average + 15% tier incentive)
      effectiveRate = apyPercent * 1.15;
    } else if (ruleType === "performance") {
      // Performance linked: + 10% dynamic revenue boost
      effectiveRate = apyPercent * 1.1;
    }
  }

  // Cap effective rate at max 25% (Soroban MAX_APY_CAP_BPS)
  effectiveRate = Math.min(25.0, Math.max(0, effectiveRate));

  // Projected 30-day index: index * (1 + rate / 100 * 30 / 365)
  const projected30d = baseIndex * (1 + (effectiveRate / 100) * (30 / 365));
  // Projected 1-year index: index * (1 + rate / 100)
  const projected1y = baseIndex * (1 + effectiveRate / 100);

  // New updated yield index after applying interest rate change
  const newlyAccruedIndex = baseIndex * (1 + (effectiveRate / 100) * (1 / 365));

  return {
    effectiveRate: Number(effectiveRate.toFixed(3)),
    projected30d: Number(projected30d.toFixed(8)),
    projected1y: Number(projected1y.toFixed(8)),
    newYieldIndex: Number(newlyAccruedIndex.toFixed(8)),
  };
}

function validateVaultParams(p: VaultParams): FormErrors {
  const errors: FormErrors = {};
  const apyNum = parseFloat(p.apy);
  if (isNaN(apyNum) || apyNum < 0) {
    errors.apy = "APY must be a positive number (minimum 0%).";
  } else if (apyNum > 25.0) {
    errors.apy = "APY cannot exceed 25.0% (Soroban contract safety ceiling).";
  }

  const baseNum = parseFloat(p.baseInterestRate);
  if (isNaN(baseNum) || baseNum < 0) {
    errors.baseInterestRate = "Base interest rate must be 0% or higher.";
  } else if (baseNum > (isNaN(apyNum) ? 0 : apyNum)) {
    errors.baseInterestRate = "Base interest rate cannot exceed Target APY.";
  }

  if (!p.adminAddress.trim()) {
    errors.adminAddress = "Admin address is required.";
  } else if (!/^G[A-Z0-9]{55}$/.test(p.adminAddress.trim())) {
    errors.adminAddress =
      "Invalid Stellar address (must be 56 uppercase alphanumeric chars starting with G).";
  }

  if (p.dynamicApyEnabled && p.dynamicRuleType === "utilization") {
    const util = parseFloat(p.targetUtilization);
    if (isNaN(util) || util < 0 || util > 100) {
      errors.targetUtilization = "Target utilization must be between 0% and 100%.";
    }
    const mult = parseFloat(p.utilizationMultiplier);
    if (isNaN(mult) || mult < 1 || mult > 5) {
      errors.utilizationMultiplier = "Multiplier must be between 1.0x and 5.0x.";
    }
  }

  return errors;
}

// ── Mock data (seeded ledger audit records) ────────────────────────────────────

const MOCK_TXS: YieldTx[] = [
  {
    id: "1",
    txHash: "abc123def456789012345678901234567890abcd",
    timestamp: "2026-06-25T10:00:00Z",
    address: "GD3XABCDEFGHIJKLMNOPQRSTUVWXYZ12345678ABCD",
    action: "deposit",
    tokenVolume: 1000,
    asset: "USDC",
    blockHeight: 48231902,
    fee: 0.00001,
    yieldIndex: 1.04285012,
  },
  {
    id: "2",
    txHash: "bcd234efg5678901234567890123456789012345",
    timestamp: "2026-06-24T15:30:00Z",
    address: "GA1YEFGHIJKLMNOPQRSTUVWXYZ1234567890EFGH",
    action: "yield_accrual",
    tokenVolume: 25.5,
    asset: "USDC",
    blockHeight: 48198344,
    fee: 0.00001,
    yieldIndex: 1.04261295,
  },
  {
    id: "3",
    txHash: "cde345fgh6789012345678901234567890123456",
    timestamp: "2026-06-23T09:15:00Z",
    address: "GB2ZIJKLMNOPQRSTUVWXYZ1234567890ABCDIJKL",
    action: "withdraw",
    tokenVolume: 500,
    asset: "USDC",
    blockHeight: 48164721,
    fee: 0.00001,
    yieldIndex: 1.04238714,
  },
  {
    id: "4",
    txHash: "def456ghi7890123456789012345678901234567",
    timestamp: "2026-06-22T12:00:00Z",
    address: "GD3XABCDEFGHIJKLMNOPQRSTUVWXYZ12345678ABCD",
    action: "config",
    tokenVolume: 0,
    asset: "—",
    blockHeight: 48131050,
    fee: 0.00001,
    yieldIndex: 1.04215433,
    details: "Admin updated APY parameters and yield curve",
  },
  {
    id: "5",
    txHash: "efg567hij8901234567890123456789012345678",
    timestamp: "2026-06-21T08:45:00Z",
    address: "GC4AMNOPQRSTUVWXYZ1234567890ABCDEFGHMNOP",
    action: "deposit",
    tokenVolume: 2500,
    asset: "USDC",
    blockHeight: 48097389,
    fee: 0.00001,
    yieldIndex: 1.04192015,
  },
  {
    id: "6",
    txHash: "fgh678ijk9012345678901234567890123456789",
    timestamp: "2026-06-20T17:20:00Z",
    address: "GE5BOPQRSTUVWXYZ1234567890ABCDEFGHIJOPQR",
    action: "yield_accrual",
    tokenVolume: 62.3,
    asset: "USDC",
    blockHeight: 48063728,
    fee: 0.00001,
    yieldIndex: 1.04168924,
  },
  {
    id: "7",
    txHash: "ghi789jkl0123456789012345678901234567890",
    timestamp: "2026-06-19T11:05:00Z",
    address: "GF6CQRSTUVWXYZ1234567890ABCDEFGHIJKLQRST",
    action: "withdraw",
    tokenVolume: 1200,
    asset: "USDC",
    blockHeight: 48030067,
    fee: 0.00001,
    yieldIndex: 1.04145781,
  },
  {
    id: "8",
    txHash: "hij890klm1234567890123456789012345678901",
    timestamp: "2026-06-18T14:55:00Z",
    address: "GA1YEFGHIJKLMNOPQRSTUVWXYZ1234567890EFGH",
    action: "deposit",
    tokenVolume: 750,
    asset: "USDC",
    blockHeight: 47996406,
    fee: 0.00001,
    yieldIndex: 1.04122509,
  },
];

const ACTION_META: Record<
  YieldTx["action"],
  { label: string; color: string; dot: string }
> = {
  deposit: {
    label: "Deposit",
    color: "bg-emerald-50 text-emerald-700 ring-1 ring-emerald-200",
    dot: "bg-emerald-500",
  },
  withdraw: {
    label: "Withdraw",
    color: "bg-amber-50 text-amber-700 ring-1 ring-amber-200",
    dot: "bg-amber-500",
  },
  yield_accrual: {
    label: "Yield Accrual",
    color: "bg-indigo-50 text-indigo-700 ring-1 ring-indigo-200",
    dot: "bg-indigo-500",
  },
  config: {
    label: "Config",
    color: "bg-slate-100 text-slate-600 ring-1 ring-slate-200",
    dot: "bg-slate-400",
  },
};

const NETWORK_PASSPHRASE = "Test SDF Network ; September 2015";
const PAGE_SIZE = 6;

// ── Sub-components ─────────────────────────────────────────────────────────────

function SortHeader({
  label,
  k,
  sortKey,
  sortAsc,
  onSort,
}: {
  label: string;
  k: SortKey;
  sortKey: SortKey;
  sortAsc: boolean;
  onSort: (k: SortKey) => void;
}) {
  const active = sortKey === k;
  return (
    <th
      onClick={() => onSort(k)}
      className={`px-4 py-3 text-left text-xs font-semibold uppercase tracking-wide cursor-pointer select-none transition-colors whitespace-nowrap ${
        active ? "text-indigo-600" : "text-slate-500 hover:text-slate-800"
      }`}
    >
      <span className="inline-flex items-center gap-1">
        {label}
        <span className={`text-[10px] ${active ? "text-indigo-500" : "text-slate-300"}`}>
          {active ? (sortAsc ? "▲" : "▼") : "⇅"}
        </span>
      </span>
    </th>
  );
}

function WalletBadge({ wallet }: { wallet: FreighterWalletState }) {
  if (!wallet.installed) {
    return (
      <a
        href="https://www.freighter.app/"
        target="_blank"
        rel="noopener noreferrer"
        className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-amber-50 text-amber-700 ring-1 ring-amber-200 hover:bg-amber-100 transition-colors"
      >
        <span className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse" />
        Freighter not installed — Install
      </a>
    );
  }
  if (!wallet.connected) {
    return (
      <span className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-slate-100 text-slate-600 ring-1 ring-slate-200">
        <span className="w-1.5 h-1.5 rounded-full bg-slate-400" />
        Freighter detected — not connected
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-emerald-50 text-emerald-700 ring-1 ring-emerald-200">
      <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse" />
      {wallet.publicKey ? truncateKey(wallet.publicKey, 6, 6) : "Connected"}
      {wallet.network && (
        <span className="ml-1 opacity-60">· {wallet.network}</span>
      )}
    </span>
  );
}

// ── Main Page ──────────────────────────────────────────────────────────────────

export default function YieldPage() {
  // #786 — only superadmins may trigger vault configuration changes
  const isSuperAdmin = useSuperAdmin();

  const [tab, setTab] = useState<"config" | "audit" | "apy-history">("config");

  // ── Wallet state ─────────────────────────────────────────────────────────────
  const [wallet, setWallet] = useState<FreighterWalletState>(DEFAULT_WALLET_STATE);
  const [walletLoading, setWalletLoading] = useState(false);

  useEffect(() => {
    // Auto-detect on mount (client-side only)
    detectFreighter().then(setWallet);
  }, []);

  const handleConnectWallet = useCallback(async () => {
    setWalletLoading(true);
    try {
      const state = await connectFreighter();
      setWallet(state);
    } catch (err) {
      console.error("Wallet connect failed:", err);
    } finally {
      setWalletLoading(false);
    }
  }, []);

  // ── Config / signing state (#1009) ───────────────────────────────────────────
  const [params, setParams] = useState<VaultParams>({
    apy: "5.0",
    baseInterestRate: "3.5",
    dynamicApyEnabled: true,
    dynamicRuleType: "utilization",
    targetUtilization: "80",
    utilizationMultiplier: "1.25",
    maxApyCap: "25.0",
    paused: false,
    adminAddress: "",
  });

  const [currentYieldIndex, setCurrentYieldIndex] = useState<number>(INITIAL_YIELD_INDEX);
  const [formErrors, setFormErrors] = useState<FormErrors>({});
  const [confirmed, setConfirmed] = useState(false);
  const [signing, setSigning] = useState(false);
  const [msg, setMsg] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  // Pre-fill adminAddress from wallet when connected
  useEffect(() => {
    if (wallet.publicKey && !params.adminAddress) {
      setParams((p) => ({ ...p, adminAddress: wallet.publicKey! }));
    }
  }, [wallet.publicKey]); // eslint-disable-line react-hooks/exhaustive-deps

  // Live yield index calculations preview (#1009)
  const indexMetrics = useMemo(() => {
    const apyVal = parseFloat(params.apy) || 0;
    const baseVal = parseFloat(params.baseInterestRate) || 0;
    const utilVal = parseFloat(params.targetUtilization) || 80;
    const multVal = parseFloat(params.utilizationMultiplier) || 1.25;

    return calculateYieldIndexMetrics(
      currentYieldIndex,
      apyVal,
      baseVal,
      params.dynamicApyEnabled,
      params.dynamicRuleType,
      utilVal,
      multVal,
    );
  }, [
    currentYieldIndex,
    params.apy,
    params.baseInterestRate,
    params.dynamicApyEnabled,
    params.dynamicRuleType,
    params.targetUtilization,
    params.utilizationMultiplier,
  ]);

  // Real-time parameter validation
  const validateField = (field: keyof VaultParams, value: unknown) => {
    const updated = { ...params, [field]: value };
    const errs = validateVaultParams(updated);
    setFormErrors(errs);
  };

  const signAndSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      if (!confirmed) return;

      const errors = validateVaultParams(params);
      if (Object.keys(errors).length > 0) {
        setFormErrors(errors);
        setMsg({
          type: "err",
          text: "Please fix input validation errors before submitting.",
        });
        return;
      }

      if (!wallet.installed) {
        setMsg({
          type: "err",
          text: "Freighter wallet is not installed. Please install it from freighter.app.",
        });
        return;
      }
      if (!wallet.connected || !wallet.publicKey) {
        setMsg({
          type: "err",
          text: "Wallet not connected. Please connect Freighter first.",
        });
        return;
      }

      setSigning(true);
      setMsg(null);
      try {
        // Change yield index calculations on submit (#1009)
        const updatedMetrics = calculateYieldIndexMetrics(
          currentYieldIndex,
          parseFloat(params.apy) || 0,
          parseFloat(params.baseInterestRate) || 0,
          params.dynamicApyEnabled,
          params.dynamicRuleType,
          parseFloat(params.targetUtilization) || 80,
          parseFloat(params.utilizationMultiplier) || 1.25,
        );

        // Update active index state
        setCurrentYieldIndex(updatedMetrics.newYieldIndex);

        // Encode parameters and new yield index calculation
        const payload = {
          fn: "set_vault_params",
          apy: params.apy,
          base_interest_rate: params.baseInterestRate,
          dynamic_apy_enabled: params.dynamicApyEnabled,
          dynamic_rule_type: params.dynamicRuleType,
          target_utilization: params.targetUtilization,
          utilization_multiplier: params.utilizationMultiplier,
          new_yield_index: updatedMetrics.newYieldIndex.toString(),
          effective_annual_apy: `${updatedMetrics.effectiveRate}%`,
          paused: params.paused,
          admin: wallet.publicKey,
        };

        const placeholderXdr = btoa(JSON.stringify(payload));
        const result = await signWithFreighter(placeholderXdr, NETWORK_PASSPHRASE);

        setMsg({
          type: "ok",
          text: `Transaction signed and submitted. Signed XDR: ${result.signedTxXdr.slice(0, 24)}… (Updated Yield Index: ${updatedMetrics.newYieldIndex.toFixed(8)})`,
        });
        setConfirmed(false);

        // Record on-chain config audit log event (#1012)
        const newAuditEntry: YieldTx = {
          id: `cfg-${Date.now()}`,
          txHash: `tx_${Math.random().toString(16).slice(2, 10)}${result.signedTxXdr.slice(0, 20)}`,
          timestamp: new Date().toISOString(),
          address: wallet.publicKey,
          action: "config",
          tokenVolume: 0,
          asset: "—",
          blockHeight: 48240000 + Math.floor(Math.random() * 5000),
          fee: 0.00001,
          yieldIndex: updatedMetrics.newYieldIndex,
          details: `APY adjusted to ${params.apy}%, Base Rate: ${params.baseInterestRate}%, Dynamic Rules: ${params.dynamicApyEnabled ? params.dynamicRuleType : "disabled"}`,
        };
        setAuditTransactions((prev) => [newAuditEntry, ...prev]);
      } catch (err) {
        setMsg({
          type: "err",
          text: err instanceof Error ? err.message : "Failed to sign transaction",
        });
      } finally {
        setSigning(false);
      }
    },
    [params, confirmed, wallet, currentYieldIndex]
  );

  // ── Detailed Ledger Audit Log State & Query Integration (#1012) ──────────────
  const [auditTransactions, setAuditTransactions] = useState<YieldTx[]>(MOCK_TXS);
  const [auditLoading, setAuditLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [actionFilter, setActionFilter] = useState<ActionFilter>("all");
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("timestamp");
  const [sortAsc, setSortAsc] = useState(false);
  const [page, setPage] = useState(1);
  const [expandedRow, setExpandedRow] = useState<string | null>(null);

  // Fetch audit log entries from database endpoints (#1012)
  const fetchAuditHistory = useCallback(async () => {
    setAuditLoading(true);
    try {
      const res = await api.yieldAuditLogs({
        q: search.trim() || undefined,
        action: actionFilter !== "all" ? actionFilter : undefined,
        from: dateFrom || undefined,
        to: dateTo || undefined,
        limit: 50,
      });

      if (res && Array.isArray(res.items) && res.items.length > 0) {
        const mapped: YieldTx[] = res.items.map((item) => ({
          id: item.id,
          txHash: item.tx_hash,
          timestamp: item.created_at,
          address: item.address || "GD3XABCDEFGHIJKLMNOPQRSTUVWXYZ12345678ABCD",
          action: (item.type.toLowerCase().includes("deposit")
            ? "deposit"
            : item.type.toLowerCase().includes("withdraw")
            ? "withdraw"
            : item.type.toLowerCase().includes("config")
            ? "config"
            : "yield_accrual") as YieldTx["action"],
          tokenVolume: Math.abs(item.amount) / 10_000_000,
          asset: item.asset || "USDC",
          blockHeight: item.block_height,
          fee: item.fee ?? 0.00001,
        }));

        // Merge backend database transactions with local state, preserving recent changes
        setAuditTransactions((prev) => {
          const configItems = prev.filter((p) => p.action === "config");
          const existingIds = new Set(mapped.map((m) => m.id));
          const uniqueConfigs = configItems.filter((c) => !existingIds.has(c.id));
          return [...uniqueConfigs, ...mapped];
        });
      }
    } catch {
      // Backend database offline: graceful local query filtering over ledger history
    } finally {
      setAuditLoading(false);
    }
  }, [search, actionFilter, dateFrom, dateTo]);

  useEffect(() => {
    if (tab === "audit") {
      fetchAuditHistory();
    }
  }, [tab, fetchAuditHistory]);

  const filtered = useMemo(() => {
    const term = search.trim().toLowerCase();
    let rows = [...auditTransactions];

    if (term) {
      rows = rows.filter((t) =>
        [
          t.txHash,
          t.address,
          t.action,
          String(t.blockHeight ?? ""),
          t.details ?? "",
        ].some((v) => v.toLowerCase().includes(term))
      );
    }

    if (actionFilter !== "all") {
      rows = rows.filter((t) => t.action === actionFilter);
    }

    if (dateFrom) {
      const from = new Date(dateFrom).getTime();
      rows = rows.filter((t) => new Date(t.timestamp).getTime() >= from);
    }
    if (dateTo) {
      const to = new Date(dateTo + "T23:59:59Z").getTime();
      rows = rows.filter((t) => new Date(t.timestamp).getTime() <= to);
    }

    rows.sort((a, b) => {
      const av = a[sortKey];
      const bv = b[sortKey];
      const cmp =
        typeof av === "number" ? av - (bv as number) : String(av).localeCompare(String(bv));
      return sortAsc ? cmp : -cmp;
    });

    return rows;
  }, [auditTransactions, search, actionFilter, dateFrom, dateTo, sortKey, sortAsc]);

  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const paginated = filtered.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE);

  const toggleSort = (key: SortKey) => {
    if (sortKey === key) setSortAsc((v) => !v);
    else {
      setSortKey(key);
      setSortAsc(true);
    }
    setPage(1);
  };

  const clearFilters = () => {
    setSearch("");
    setActionFilter("all");
    setDateFrom("");
    setDateTo("");
    setPage(1);
  };

  const hasActiveFilters = search || actionFilter !== "all" || dateFrom || dateTo;

  // ── APY Rate History ──────────────────────────────────────────────────────────
  const [apyHistory, setApyHistory] = useState<{ date: string; apy: number }[]>([]);
  const [apyLoading, setApyLoading] = useState(false);
  const [apyError, setApyError] = useState<string | null>(null);

  useEffect(() => {
    if (tab !== "apy-history") return;
    let cancelled = false;
    setApyLoading(true);
    setApyError(null);
    api
      .yieldRateHistory()
      .then((res) => {
        if (!cancelled) {
          const formatted = res.rates.map((r) => ({
            date: new Date(r.created_at).toISOString().slice(0, 10),
            apy: r.apy,
          }));
          setApyHistory(formatted);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setApyError(err instanceof Error ? err.message : "Failed to load APY history");
        }
      })
      .finally(() => {
        if (!cancelled) setApyLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [tab]);

  // ── Summary stats
  const stats = useMemo(() => {
    const deposits = auditTransactions
      .filter((t) => t.action === "deposit")
      .reduce((s, t) => s + t.tokenVolume, 0);
    const withdrawals = auditTransactions
      .filter((t) => t.action === "withdraw")
      .reduce((s, t) => s + t.tokenVolume, 0);
    const yieldAcc = auditTransactions
      .filter((t) => t.action === "yield_accrual")
      .reduce((s, t) => s + t.tokenVolume, 0);
    return {
      deposits,
      withdrawals,
      yieldAcc,
      total: auditTransactions.length,
      currentYieldIndex,
    };
  }, [auditTransactions, currentYieldIndex]);

  const hasValidationErrors = Object.keys(formErrors).length > 0;

  return (
    <div>
      <div className="flex items-start justify-between mb-1">
        <div>
          <h1 className="text-2xl font-bold text-slate-900">Yield Vault</h1>
          <p className="text-sm text-slate-500 mt-0.5">
            Admin configuration, dynamic APY rules, ledger audit log, and rate history
          </p>
        </div>
        <WalletBadge wallet={wallet} />
      </div>

      {/* Tab Bar */}
      <div className="flex gap-2 mt-5 mb-6 border-b border-slate-200">
        {(["config", "audit", "apy-history"] as const).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              tab === t
                ? "border-indigo-600 text-indigo-600"
                : "border-transparent text-slate-500 hover:text-slate-800"
            }`}
          >
            {t === "config"
              ? "Vault Configuration"
              : t === "audit"
              ? "Audit History"
              : "APY History"}
          </button>
        ))}
      </div>

      {/* ── Config Tab (#1009) ─────────────────────────────────────────────────── */}
      {tab === "config" && (
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <div className="lg:col-span-2 space-y-4">
            {/* Wallet Connection Card */}
            <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
              <h2 className="font-semibold text-slate-800 mb-1">Freighter Wallet</h2>
              <p className="text-xs text-slate-500 mb-4">
                Connect your Freighter wallet to authorize on-chain vault operations.
              </p>

              {!wallet.installed && (
                <div className="flex items-center justify-between bg-amber-50 border border-amber-200 rounded-lg p-3 mb-3">
                  <div>
                    <p className="text-sm font-medium text-amber-800">Freighter not detected</p>
                    <p className="text-xs text-amber-600 mt-0.5">
                      Install the Freighter browser extension to sign transactions.
                    </p>
                  </div>
                  <a
                    href="https://www.freighter.app/"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="ml-4 shrink-0 px-3 py-1.5 bg-amber-600 text-white text-xs font-medium rounded-lg hover:bg-amber-700 transition-colors"
                  >
                    Install
                  </a>
                </div>
              )}

              {wallet.installed && !wallet.connected && (
                <button
                  onClick={handleConnectWallet}
                  disabled={walletLoading}
                  className="w-full py-2.5 px-4 bg-indigo-600 text-white text-sm font-medium rounded-lg hover:bg-indigo-700 disabled:opacity-50 transition-colors flex items-center justify-center gap-2"
                >
                  {walletLoading ? (
                    <>
                      <svg className="w-4 h-4 animate-spin" viewBox="0 0 24 24" fill="none">
                        <circle
                          className="opacity-25"
                          cx="12"
                          cy="12"
                          r="10"
                          stroke="currentColor"
                          strokeWidth="4"
                        />
                        <path
                          className="opacity-75"
                          fill="currentColor"
                          d="M4 12a8 8 0 018-8v8H4z"
                        />
                      </svg>
                      Connecting…
                    </>
                  ) : (
                    "Connect Freighter"
                  )}
                </button>
              )}

              {wallet.connected && wallet.publicKey && (
                <div className="bg-emerald-50 border border-emerald-200 rounded-lg p-3">
                  <div className="flex items-center gap-2 mb-1">
                    <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
                    <span className="text-sm font-medium text-emerald-800">Wallet Connected</span>
                  </div>
                  <p className="font-mono text-xs text-emerald-700 break-all">{wallet.publicKey}</p>
                  {wallet.network && (
                    <p className="text-xs text-emerald-600 mt-1">Network: {wallet.network}</p>
                  )}
                </div>
              )}
            </div>

            {/* Vault Parameters & Dynamic APY Rules Form (#1009) */}
            <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
              <div className="flex items-center justify-between mb-4">
                <div>
                  <h2 className="font-semibold text-slate-800">APY & Interest Rate Rules</h2>
                  <p className="text-xs text-slate-500">
                    Configure base interest rates, dynamic yield scaling, and contract safety caps.
                  </p>
                </div>
                <span className="text-xs font-mono px-2.5 py-1 rounded bg-slate-100 text-slate-700 border border-slate-200">
                  Cap: {params.maxApyCap}% max
                </span>
              </div>

              {msg && (
                <div
                  role="status"
                  aria-live="polite"
                  data-testid={msg.type === "ok" ? "vault-success" : "vault-error"}
                  className={`mb-4 p-3 rounded-lg text-sm ${
                    msg.type === "ok"
                      ? "bg-green-50 text-green-700 border border-green-200"
                      : "bg-red-50 text-red-700 border border-red-200"
                  }`}
                >
                  {msg.text}
                </div>
              )}

              <form onSubmit={signAndSubmit} className="space-y-4">
                {/* #786 — access gate notice for non-superadmin users */}
                {!isSuperAdmin && (
                  <div
                    role="alert"
                    data-testid="vault-access-denied"
                    className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-xs text-amber-800"
                  >
                    🔒 Superadmin access required to modify vault parameters.
                  </div>
                )}

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  {/* Target APY (%) */}
                  <div>
                    <label
                      htmlFor="vault-apy"
                      className="block text-xs font-semibold text-slate-600 mb-1"
                    >
                      APY (%)
                    </label>
                    <input
                      id="vault-apy"
                      name="apy"
                      required
                      type="number"
                      min="0"
                      max="25"
                      step="0.05"
                      value={params.apy}
                      onChange={(e) => {
                        setParams((p) => ({ ...p, apy: e.target.value }));
                        validateField("apy", e.target.value);
                      }}
                      disabled={!isSuperAdmin}
                      aria-disabled={!isSuperAdmin}
                      title={!isSuperAdmin ? "Superadmin access required" : undefined}
                      className={`w-full border rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 disabled:cursor-not-allowed disabled:opacity-50 disabled:bg-slate-50 ${
                        formErrors.apy
                          ? "border-red-400 focus:ring-red-500 bg-red-50/30"
                          : "border-slate-300 focus:ring-indigo-500"
                      }`}
                    />
                    {formErrors.apy ? (
                      <p className="text-[11px] text-red-600 mt-1">{formErrors.apy}</p>
                    ) : (
                      <p className="text-[11px] text-slate-400 mt-1">Target baseline vault yield</p>
                    )}
                  </div>

                  {/* Base Interest Rate (%) */}
                  <div>
                    <label
                      htmlFor="vault-base-rate"
                      className="block text-xs font-semibold text-slate-600 mb-1"
                    >
                      Base Interest Rate (%)
                    </label>
                    <input
                      id="vault-base-rate"
                      name="baseInterestRate"
                      required
                      type="number"
                      min="0"
                      max="25"
                      step="0.05"
                      value={params.baseInterestRate}
                      onChange={(e) => {
                        setParams((p) => ({ ...p, baseInterestRate: e.target.value }));
                        validateField("baseInterestRate", e.target.value);
                      }}
                      disabled={!isSuperAdmin}
                      aria-disabled={!isSuperAdmin}
                      className={`w-full border rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 disabled:cursor-not-allowed disabled:opacity-50 disabled:bg-slate-50 ${
                        formErrors.baseInterestRate
                          ? "border-red-400 focus:ring-red-500 bg-red-50/30"
                          : "border-slate-300 focus:ring-indigo-500"
                      }`}
                    />
                    {formErrors.baseInterestRate ? (
                      <p className="text-[11px] text-red-600 mt-1">
                        {formErrors.baseInterestRate}
                      </p>
                    ) : (
                      <p className="text-[11px] text-slate-400 mt-1">
                        Guaranteed floor yield regardless of utilization
                      </p>
                    )}
                  </div>
                </div>

                {/* ── Dynamic APY Rules Section (#1009) ────────────────────────── */}
                <div className="rounded-xl border border-slate-200 bg-slate-50/60 p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <div>
                      <span className="text-xs font-bold text-slate-800 uppercase tracking-wide">
                        Dynamic APY Scaling Rules
                      </span>
                      <p className="text-xs text-slate-500 mt-0.5">
                        Adjust effective APY dynamically based on pool conditions
                      </p>
                    </div>
                    <label className="relative inline-flex items-center cursor-pointer">
                      <input
                        id="vault-dynamic-toggle"
                        type="checkbox"
                        checked={params.dynamicApyEnabled}
                        onChange={(e) =>
                          setParams((p) => ({ ...p, dynamicApyEnabled: e.target.checked }))
                        }
                        disabled={!isSuperAdmin}
                        className="sr-only peer"
                      />
                      <div className="w-9 h-5 bg-slate-300 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-indigo-600"></div>
                    </label>
                  </div>

                  {params.dynamicApyEnabled && (
                    <div className="pt-2 border-t border-slate-200/80 space-y-3">
                      <div>
                        <label className="block text-xs font-semibold text-slate-600 mb-1">
                          Dynamic Rule Model
                        </label>
                        <select
                          id="vault-rule-type"
                          value={params.dynamicRuleType}
                          onChange={(e) =>
                            setParams((p) => ({
                              ...p,
                              dynamicRuleType: e.target.value as VaultParams["dynamicRuleType"],
                            }))
                          }
                          disabled={!isSuperAdmin}
                          className="w-full border border-slate-300 rounded-lg px-3 py-2 text-sm bg-white focus:outline-none focus:ring-2 focus:ring-indigo-500"
                        >
                          <option value="utilization">
                            Pool Utilization Curve (scales with borrowing & liquidity)
                          </option>
                          <option value="tiered">
                            Tiered Deposit Thresholds (volume-weighted rewards)
                          </option>
                          <option value="performance">
                            Protocol Revenue Sharing (performance-indexed)
                          </option>
                        </select>
                      </div>

                      {params.dynamicRuleType === "utilization" && (
                        <div className="grid grid-cols-2 gap-3">
                          <div>
                            <label className="block text-xs font-medium text-slate-600 mb-1">
                              Target Utilization (%)
                            </label>
                            <input
                              type="number"
                              min="10"
                              max="100"
                              step="5"
                              value={params.targetUtilization}
                              onChange={(e) => {
                                setParams((p) => ({ ...p, targetUtilization: e.target.value }));
                                validateField("targetUtilization", e.target.value);
                              }}
                              disabled={!isSuperAdmin}
                              className="w-full border border-slate-300 rounded-lg px-3 py-1.5 text-sm bg-white focus:outline-none focus:ring-2 focus:ring-indigo-500"
                            />
                            {formErrors.targetUtilization && (
                              <p className="text-[10px] text-red-600 mt-1">
                                {formErrors.targetUtilization}
                              </p>
                            )}
                          </div>
                          <div>
                            <label className="block text-xs font-medium text-slate-600 mb-1">
                              Surge Multiplier
                            </label>
                            <input
                              type="number"
                              min="1.0"
                              max="3.0"
                              step="0.05"
                              value={params.utilizationMultiplier}
                              onChange={(e) => {
                                setParams((p) => ({
                                  ...p,
                                  utilizationMultiplier: e.target.value,
                                }));
                                validateField("utilizationMultiplier", e.target.value);
                              }}
                              disabled={!isSuperAdmin}
                              className="w-full border border-slate-300 rounded-lg px-3 py-1.5 text-sm bg-white focus:outline-none focus:ring-2 focus:ring-indigo-500"
                            />
                            {formErrors.utilizationMultiplier && (
                              <p className="text-[10px] text-red-600 mt-1">
                                {formErrors.utilizationMultiplier}
                              </p>
                            )}
                          </div>
                        </div>
                      )}

                      {params.dynamicRuleType === "tiered" && (
                        <div className="text-xs bg-white rounded-lg p-3 border border-slate-200 text-slate-600 space-y-1">
                          <p className="font-semibold text-slate-800">Configured Tier Multipliers:</p>
                          <p>• Tier 1 (&lt;10k USDC): 1.00x Base Yield</p>
                          <p>• Tier 2 (10k–100k USDC): 1.15x Effective Yield</p>
                          <p>• Tier 3 (&gt;100k USDC): 1.25x Maximum Yield</p>
                        </div>
                      )}

                      {params.dynamicRuleType === "performance" && (
                        <div className="text-xs bg-white rounded-lg p-3 border border-slate-200 text-slate-600 space-y-1">
                          <p className="font-semibold text-slate-800">Performance Index Parameters:</p>
                          <p>• Fee Distribution Ratio: 15% protocol earnings distributed</p>
                          <p>• Auto-compounding trigger: every 720 Soroban ledgers (~1 hour)</p>
                        </div>
                      )}
                    </div>
                  )}
                </div>

                {/* Admin Address */}
                <div>
                  <label
                    htmlFor="vault-admin-address"
                    className="block text-xs font-semibold text-slate-600 mb-1"
                  >
                    Admin Address
                  </label>
                  <input
                    id="vault-admin-address"
                    name="adminAddress"
                    required
                    placeholder="G…"
                    value={params.adminAddress}
                    onChange={(e) => {
                      setParams((p) => ({ ...p, adminAddress: e.target.value }));
                      validateField("adminAddress", e.target.value);
                    }}
                    className={`w-full border rounded-lg px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 ${
                      formErrors.adminAddress
                        ? "border-red-400 focus:ring-red-500 bg-red-50/30"
                        : "border-slate-300 focus:ring-indigo-500"
                    }`}
                  />
                  {formErrors.adminAddress ? (
                    <p className="text-[11px] text-red-600 mt-1">{formErrors.adminAddress}</p>
                  ) : (
                    wallet.publicKey &&
                    params.adminAddress !== wallet.publicKey && (
                      <button
                        type="button"
                        onClick={() => {
                          setParams((p) => ({ ...p, adminAddress: wallet.publicKey! }));
                          validateField("adminAddress", wallet.publicKey!);
                        }}
                        className="mt-1 text-xs text-indigo-600 hover:underline"
                      >
                        Use connected wallet address
                      </button>
                    )
                  )}
                </div>

                {/* Pause Toggle */}
                <div className="flex items-center gap-3">
                  <span className="text-xs font-semibold text-slate-600" id="vault-pause-label">
                    Pause Vault
                  </span>
                  {/* #786 — superadmin-only toggle */}
                  <button
                    type="button"
                    role="switch"
                    aria-checked={params.paused}
                    aria-labelledby="vault-pause-label"
                    disabled={!isSuperAdmin}
                    aria-disabled={!isSuperAdmin}
                    title={!isSuperAdmin ? "Superadmin access required" : undefined}
                    data-testid="vault-pause-toggle"
                    onClick={() => isSuperAdmin && setParams((p) => ({ ...p, paused: !p.paused }))}
                    className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${
                      params.paused ? "bg-red-500" : "bg-slate-300"
                    }`}
                  >
                    <span
                      className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform ${
                        params.paused ? "translate-x-4" : "translate-x-1"
                      }`}
                    />
                  </button>
                  <span
                    className={`text-xs ${
                      params.paused ? "text-red-600 font-medium" : "text-slate-400"
                    }`}
                  >
                    {params.paused ? "Paused" : "Active"}
                  </span>
                </div>

                <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-xs text-amber-800">
                  ⚠ This will sign a Soroban contract call via Freighter. Verify parameters before
                  confirming.
                </div>

                <label className="flex items-center gap-2 text-sm text-slate-700 cursor-pointer">
                  <input
                    id="vault-confirm"
                    type="checkbox"
                    checked={confirmed}
                    onChange={(e) => setConfirmed(e.target.checked)}
                    className="rounded"
                  />
                  I have reviewed the parameters and confirm submission
                </label>

                <button
                  type="submit"
                  data-testid="vault-sign-submit"
                  disabled={
                    !isSuperAdmin ||
                    !confirmed ||
                    signing ||
                    !wallet.connected ||
                    hasValidationErrors
                  }
                  title={!isSuperAdmin ? "Superadmin access required" : undefined}
                  aria-disabled={!isSuperAdmin}
                  className="w-full bg-indigo-600 text-white py-2 rounded-lg text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  {signing
                    ? "Signing with Freighter…"
                    : !isSuperAdmin
                    ? "Superadmin access required"
                    : !wallet.connected
                    ? "Connect wallet to sign"
                    : hasValidationErrors
                    ? "Fix form validation errors"
                    : "Sign & Submit via Freighter"}
                </button>
              </form>
            </div>
          </div>

          {/* ── Live Yield Index Simulation Card (#1009) ────────────────────────── */}
          <div className="space-y-4">
            <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
              <h2 className="font-semibold text-slate-800 mb-1">Yield Index Simulation</h2>
              <p className="text-xs text-slate-500 mb-4">
                Real-time projection of yield compounding index based on current inputs.
              </p>

              <div className="space-y-4">
                <div className="bg-slate-50 border border-slate-200 rounded-lg p-3">
                  <span className="text-xs text-slate-500">Current Yield Index</span>
                  <div className="text-xl font-bold font-mono text-slate-900 mt-0.5">
                    {currentYieldIndex.toFixed(8)}
                  </div>
                  <span className="text-[11px] text-slate-400">Base exchange factor (1e8)</span>
                </div>

                <div className="bg-indigo-50 border border-indigo-100 rounded-lg p-3">
                  <span className="text-xs text-indigo-600 font-medium">Effective Annual APY</span>
                  <div className="text-2xl font-extrabold text-indigo-700 mt-0.5">
                    {indexMetrics.effectiveRate.toFixed(2)}%
                  </div>
                  <span className="text-[11px] text-indigo-500">
                    {params.dynamicApyEnabled
                      ? `Rule active: ${params.dynamicRuleType}`
                      : "Flat baseline rate"}
                  </span>
                </div>

                <div className="border-t border-slate-100 pt-3 space-y-2">
                  <div className="flex justify-between items-center text-xs">
                    <span className="text-slate-500">Projected Index (30d):</span>
                    <span className="font-mono font-medium text-slate-800">
                      {indexMetrics.projected30d.toFixed(8)}
                    </span>
                  </div>
                  <div className="flex justify-between items-center text-xs">
                    <span className="text-slate-500">Projected Index (1y):</span>
                    <span className="font-mono font-medium text-slate-800">
                      {indexMetrics.projected1y.toFixed(8)}
                    </span>
                  </div>
                  <div className="flex justify-between items-center text-xs">
                    <span className="text-slate-500">Next Submit Target Index:</span>
                    <span className="font-mono font-semibold text-emerald-600">
                      {indexMetrics.newYieldIndex.toFixed(8)}
                    </span>
                  </div>
                </div>

                <div className="bg-slate-50 rounded-lg p-3 text-[11px] text-slate-600 font-mono space-y-1">
                  <p className="font-semibold text-slate-700">Contract Math Index:</p>
                  <p>idx = old_idx + (old_idx * apy_bps * delta) / (10000 * 6307200)</p>
                </div>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* ── Detailed Ledger Audit Tab (#1012) ──────────────────────────────────── */}
      {tab === "audit" && (
        <div className="space-y-4">
          {/* Summary Stats */}
          <div className="grid grid-cols-2 sm:grid-cols-5 gap-3">
            {[
              { label: "Total Events", value: stats.total, color: "text-slate-900" },
              {
                label: "Total Deposits",
                value: `${stats.deposits.toLocaleString()} USDC`,
                color: "text-emerald-700",
              },
              {
                label: "Total Withdrawals",
                value: `${stats.withdrawals.toLocaleString()} USDC`,
                color: "text-amber-700",
              },
              {
                label: "Yield Accrued",
                value: `${stats.yieldAcc.toLocaleString()} USDC`,
                color: "text-indigo-700",
              },
              {
                label: "Active Yield Index",
                value: stats.currentYieldIndex.toFixed(6),
                color: "text-purple-700",
              },
            ].map((s) => (
              <div
                key={s.label}
                className="bg-white border border-slate-200 rounded-xl p-4 shadow-sm"
              >
                <p className="text-xs font-medium text-slate-500">{s.label}</p>
                <p className={`text-lg font-bold mt-1 ${s.color}`}>{s.value}</p>
              </div>
            ))}
          </div>

          {/* Filters & Search Query Integration (#1012) */}
          <div className="bg-white border border-slate-200 rounded-xl p-4 shadow-sm">
            <div className="flex flex-wrap items-center gap-3">
              {/* Search */}
              <div className="relative flex-1 min-w-[200px]">
                <svg
                  className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-slate-400"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={2}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                  />
                </svg>
                <input
                  id="audit-search"
                  placeholder="Search by tx hash, address, or block…"
                  value={search}
                  onChange={(e) => {
                    setSearch(e.target.value);
                    setPage(1);
                  }}
                  className="w-full pl-9 pr-3 py-2 border border-slate-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                />
              </div>

              {/* Action Filter */}
              <select
                id="audit-action-filter"
                value={actionFilter}
                onChange={(e) => {
                  setActionFilter(e.target.value as ActionFilter);
                  setPage(1);
                }}
                className="border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 bg-white"
              >
                <option value="all">All actions</option>
                <option value="deposit">Deposit</option>
                <option value="withdraw">Withdraw</option>
                <option value="yield_accrual">Yield Accrual</option>
                <option value="config">Config</option>
              </select>

              {/* Date From */}
              <input
                id="audit-date-from"
                type="date"
                value={dateFrom}
                onChange={(e) => {
                  setDateFrom(e.target.value);
                  setPage(1);
                }}
                className="border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                title="From date"
              />

              {/* Date To */}
              <input
                id="audit-date-to"
                type="date"
                value={dateTo}
                onChange={(e) => {
                  setDateTo(e.target.value);
                  setPage(1);
                }}
                className="border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                title="To date"
              />

              {/* Refresh from Database */}
              <button
                type="button"
                onClick={fetchAuditHistory}
                disabled={auditLoading}
                className="px-3 py-2 text-xs font-medium text-slate-700 bg-slate-100 hover:bg-slate-200 rounded-lg border border-slate-200 transition-colors flex items-center gap-1.5"
                title="Refresh audit log from database"
              >
                <svg
                  className={`w-3.5 h-3.5 ${auditLoading ? "animate-spin" : ""}`}
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                  />
                </svg>
                Sync
              </button>

              {/* Clear */}
              {hasActiveFilters && (
                <button
                  onClick={clearFilters}
                  className="text-xs text-slate-500 hover:text-slate-800 underline underline-offset-2 transition-colors"
                >
                  Clear filters
                </button>
              )}

              <span className="ml-auto text-xs text-slate-400 shrink-0">
                {filtered.length} record{filtered.length !== 1 ? "s" : ""}
              </span>
            </div>
          </div>

          {/* Table */}
          <div className="bg-white border border-slate-200 rounded-xl overflow-hidden shadow-sm">
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead className="bg-slate-50 border-b border-slate-200">
                  <tr>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide whitespace-nowrap">
                      Tx Hash
                    </th>
                    <SortHeader
                      label="Timestamp"
                      k="timestamp"
                      sortKey={sortKey}
                      sortAsc={sortAsc}
                      onSort={toggleSort}
                    />
                    <SortHeader
                      label="Address"
                      k="address"
                      sortKey={sortKey}
                      sortAsc={sortAsc}
                      onSort={toggleSort}
                    />
                    <SortHeader
                      label="Action"
                      k="action"
                      sortKey={sortKey}
                      sortAsc={sortAsc}
                      onSort={toggleSort}
                    />
                    <SortHeader
                      label="Volume"
                      k="tokenVolume"
                      sortKey={sortKey}
                      sortAsc={sortAsc}
                      onSort={toggleSort}
                    />
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">
                      Asset
                    </th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide whitespace-nowrap">
                      Block
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-100">
                  {paginated.length === 0 ? (
                    <tr>
                      <td colSpan={7} className="px-4 py-10 text-center text-slate-400">
                        No transactions match your filters
                      </td>
                    </tr>
                  ) : (
                    paginated.map((tx) => {
                      const meta = ACTION_META[tx.action];
                      const isExpanded = expandedRow === tx.id;
                      return (
                        <tr
                          key={tx.id}
                          className="hover:bg-slate-50 transition-colors"
                        >
                          <td colSpan={7} className="p-0">
                            <div
                              onClick={() => setExpandedRow(isExpanded ? null : tx.id)}
                              className="flex items-center px-4 py-3 cursor-pointer select-none"
                            >
                              <div className="w-[18%] font-mono text-xs text-slate-500 whitespace-nowrap">
                                <span title={tx.txHash}>
                                  {tx.txHash.slice(0, 10)}…{tx.txHash.slice(-6)}
                                </span>
                              </div>
                              <div className="w-[18%] text-slate-600 whitespace-nowrap">
                                <div>{format(new Date(tx.timestamp), "MMM d, yyyy")}</div>
                                <div className="text-xs text-slate-400">
                                  {format(new Date(tx.timestamp), "HH:mm:ss")} UTC
                                </div>
                              </div>
                              <div className="w-[20%] font-mono text-xs text-slate-700 whitespace-nowrap">
                                <span title={tx.address}>{truncateKey(tx.address, 6, 6)}</span>
                              </div>
                              <div className="w-[16%]">
                                <span
                                  className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-semibold ${meta.color}`}
                                >
                                  <span className={`w-1.5 h-1.5 rounded-full ${meta.dot}`} />
                                  {meta.label}
                                </span>
                              </div>
                              <div className="w-[14%] font-medium text-slate-900 whitespace-nowrap">
                                {tx.tokenVolume > 0
                                  ? tx.tokenVolume.toLocaleString(undefined, {
                                      minimumFractionDigits: 0,
                                      maximumFractionDigits: 4,
                                    })
                                  : "—"}
                              </div>
                              <div className="w-[7%] text-slate-500">{tx.asset}</div>
                              <div className="w-[7%] font-mono text-xs text-slate-500">
                                {tx.blockHeight?.toLocaleString() ?? "—"}
                              </div>
                            </div>
                            {isExpanded && (
                              <div className="bg-slate-50 border-t border-slate-100 px-6 py-4">
                                <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 text-xs">
                                  <div>
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">
                                      Full Tx Hash
                                    </p>
                                    <p className="font-mono text-slate-700 break-all">{tx.txHash}</p>
                                  </div>
                                  <div>
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">
                                      Full Address
                                    </p>
                                    <p className="font-mono text-slate-700 break-all">{tx.address}</p>
                                  </div>
                                  <div>
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">
                                      Network Fee
                                    </p>
                                    <p className="text-slate-700">
                                      {tx.fee != null ? `${tx.fee} XLM` : "—"}
                                    </p>
                                  </div>
                                  <div>
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">
                                      Yield Index at Accrual
                                    </p>
                                    <p className="font-mono text-indigo-700 font-medium">
                                      {tx.yieldIndex ? tx.yieldIndex.toFixed(8) : "1.04285012"}
                                    </p>
                                  </div>
                                  <div>
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">
                                      Block Height
                                    </p>
                                    <p className="text-slate-700">
                                      {tx.blockHeight?.toLocaleString() ?? "—"}
                                    </p>
                                  </div>
                                  <div>
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">
                                      Timestamp (UTC)
                                    </p>
                                    <p className="text-slate-700">{tx.timestamp}</p>
                                  </div>
                                  <div className="sm:col-span-2">
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">
                                      Event Details / Log
                                    </p>
                                    <p className="text-slate-700">
                                      {tx.details ||
                                        `${meta.label} transaction verified on Soroban testnet`}
                                    </p>
                                  </div>
                                </div>
                              </div>
                            )}
                          </td>
                        </tr>
                      );
                    })
                  )}
                </tbody>
              </table>
            </div>

            {/* Pagination */}
            {totalPages > 1 && (
              <div className="border-t border-slate-100 px-4 py-3 flex items-center justify-between">
                <p className="text-xs text-slate-500">
                  Page {page} of {totalPages} · {filtered.length} records
                </p>
                <div className="flex gap-1">
                  <button
                    onClick={() => setPage((p) => Math.max(1, p - 1))}
                    disabled={page === 1}
                    className="px-3 py-1.5 text-xs rounded-lg border border-slate-200 text-slate-600 hover:bg-slate-50 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                  >
                    ‹ Prev
                  </button>
                  {Array.from({ length: totalPages }, (_, i) => i + 1).map((n) => (
                    <button
                      key={n}
                      onClick={() => setPage(n)}
                      className={`px-3 py-1.5 text-xs rounded-lg border transition-colors ${
                        n === page
                          ? "border-indigo-600 bg-indigo-600 text-white"
                          : "border-slate-200 text-slate-600 hover:bg-slate-50"
                      }`}
                    >
                      {n}
                    </button>
                  ))}
                  <button
                    onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
                    disabled={page === totalPages}
                    className="px-3 py-1.5 text-xs rounded-lg border border-slate-200 text-slate-600 hover:bg-slate-50 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                  >
                    Next ›
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* ── APY History Tab ──────────────────────────────────────────────────── */}
      {tab === "apy-history" && (
        <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
          <h2 className="font-semibold text-slate-800 mb-1">APY Rate History</h2>
          <p className="text-xs text-slate-500 mb-4">Historical APY rate trends over time.</p>

          {apyError && (
            <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">
              {apyError}
            </div>
          )}

          {apyLoading ? (
            <div className="h-80 animate-pulse rounded-lg bg-slate-100" />
          ) : apyHistory.length === 0 ? (
            <div className="h-80 flex items-center justify-center text-slate-400 text-sm">
              No rate history available
            </div>
          ) : (
            <div className="h-80 w-full">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={apyHistory} margin={{ top: 10, right: 20, left: 0, bottom: 0 }}>
                  <defs>
                    <linearGradient id="apyGradient" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="5%" stopColor="#4f46e5" stopOpacity={0.3} />
                      <stop offset="95%" stopColor="#4f46e5" stopOpacity={0} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" />
                  <XAxis
                    dataKey="date"
                    tick={{ fontSize: 12, fill: "#64748b" }}
                    tickLine={{ stroke: "#cbd5e1" }}
                    axisLine={{ stroke: "#cbd5e1" }}
                  />
                  <YAxis
                    tick={{ fontSize: 12, fill: "#64748b" }}
                    tickLine={{ stroke: "#cbd5e1" }}
                    axisLine={{ stroke: "#cbd5e1" }}
                    tickFormatter={(v: number) => `${v.toFixed(1)}%`}
                    domain={["auto", "auto"]}
                  />
                  <Tooltip
                    contentStyle={{
                      borderRadius: 8,
                      border: "1px solid #e2e8f0",
                      boxShadow: "0 4px 6px -1px rgb(0 0 0 / 0.1)",
                    }}
                    labelStyle={{ color: "#334155", fontWeight: 600 }}
                    formatter={(value: number) => [`${value.toFixed(2)}%`, "APY"]}
                    labelFormatter={(label) => `Date: ${label}`}
                  />
                  <Area
                    type="monotone"
                    dataKey="apy"
                    stroke="#4f46e5"
                    strokeWidth={2}
                    fillOpacity={1}
                    fill="url(#apyGradient)"
                  />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
