"use client";
import { useState, useMemo, useCallback, useEffect } from "react";
import { format } from "date-fns";
import {
  detectFreighter,
  connectFreighter,
  signWithFreighter,
  truncateKey,
  type FreighterWalletState,
  DEFAULT_WALLET_STATE,
} from "@/lib/freighter";

// ── Types ──────────────────────────────────────────────────────────────────────

interface VaultParams {
  apy: string;
  paused: boolean;
  adminAddress: string;
}

interface YieldTx {
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
}

type SortKey = keyof Pick<YieldTx, "timestamp" | "address" | "action" | "tokenVolume">;
type ActionFilter = YieldTx["action"] | "all";

// ── Mock data (replace with real API calls) ────────────────────────────────────

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
  const [tab, setTab] = useState<"config" | "audit">("config");

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

  // ── Config / signing state ────────────────────────────────────────────────────
  const [params, setParams] = useState<VaultParams>({
    apy: "5.0",
    paused: false,
    adminAddress: "",
  });
  const [confirmed, setConfirmed] = useState(false);
  const [signing, setSigning] = useState(false);
  const [msg, setMsg] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  // Pre-fill adminAddress from wallet when connected
  useEffect(() => {
    if (wallet.publicKey && !params.adminAddress) {
      setParams((p) => ({ ...p, adminAddress: wallet.publicKey! }));
    }
  }, [wallet.publicKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const signAndSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      if (!confirmed) return;

      if (!wallet.installed) {
        setMsg({ type: "err", text: "Freighter wallet is not installed. Please install it from freighter.app." });
        return;
      }
      if (!wallet.connected || !wallet.publicKey) {
        setMsg({ type: "err", text: "Wallet not connected. Please connect Freighter first." });
        return;
      }

      setSigning(true);
      setMsg(null);
      try {
        const placeholderXdr = btoa(
          JSON.stringify({ fn: "set_vault_params", ...params, admin: wallet.publicKey })
        );
        const result = await signWithFreighter(placeholderXdr, NETWORK_PASSPHRASE);
        setMsg({
          type: "ok",
          text: `Transaction signed and submitted. Signed XDR: ${result.signedTxXdr.slice(0, 24)}…`,
        });
        setConfirmed(false);
      } catch (err) {
        setMsg({
          type: "err",
          text: err instanceof Error ? err.message : "Failed to sign transaction",
        });
      } finally {
        setSigning(false);
      }
    },
    [params, confirmed, wallet]
  );

  // ── Audit log state ───────────────────────────────────────────────────────────
  const [search, setSearch] = useState("");
  const [actionFilter, setActionFilter] = useState<ActionFilter>("all");
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("timestamp");
  const [sortAsc, setSortAsc] = useState(false);
  const [page, setPage] = useState(1);
  const [expandedRow, setExpandedRow] = useState<string | null>(null);

  const filtered = useMemo(() => {
    const term = search.trim().toLowerCase();
    let rows = [...MOCK_TXS];

    if (term) {
      rows = rows.filter((t) =>
        [t.txHash, t.address, t.action, String(t.blockHeight ?? "")].some((v) =>
          v.toLowerCase().includes(term)
        )
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
  }, [search, actionFilter, dateFrom, dateTo, sortKey, sortAsc]);

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

  // Summary stats
  const stats = useMemo(() => {
    const deposits = MOCK_TXS.filter((t) => t.action === "deposit").reduce(
      (s, t) => s + t.tokenVolume,
      0
    );
    const withdrawals = MOCK_TXS.filter((t) => t.action === "withdraw").reduce(
      (s, t) => s + t.tokenVolume,
      0
    );
    const yieldAcc = MOCK_TXS.filter((t) => t.action === "yield_accrual").reduce(
      (s, t) => s + t.tokenVolume,
      0
    );
    return { deposits, withdrawals, yieldAcc, total: MOCK_TXS.length };
  }, []);

  return (
    <div>
      <div className="flex items-start justify-between mb-1">
        <div>
          <h1 className="text-2xl font-bold text-slate-900">Yield Vault</h1>
          <p className="text-sm text-slate-500 mt-0.5">Admin configuration and transaction audit log</p>
        </div>
        <WalletBadge wallet={wallet} />
      </div>

      {/* Tab Bar */}
      <div className="flex gap-2 mt-5 mb-6 border-b border-slate-200">
        {(["config", "audit"] as const).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              tab === t
                ? "border-indigo-600 text-indigo-600"
                : "border-transparent text-slate-500 hover:text-slate-800"
            }`}
          >
            {t === "config" ? "Vault Configuration" : "Audit History"}
          </button>
        ))}
      </div>

      {/* ── Config Tab ─────────────────────────────────────────────────────────── */}
      {tab === "config" && (
        <div className="max-w-lg space-y-4">
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
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8H4z" />
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

          {/* Vault Params Card */}
          <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
            <h2 className="font-semibold text-slate-800 mb-4">Vault Parameters</h2>

            {msg && (
              <div
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
              <div>
                <label className="block text-xs font-semibold text-slate-600 mb-1">APY (%)</label>
                <input
                  required
                  type="number"
                  min="0"
                  step="0.1"
                  value={params.apy}
                  onChange={(e) => setParams((p) => ({ ...p, apy: e.target.value }))}
                  className="w-full border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                />
              </div>

              <div>
                <label className="block text-xs font-semibold text-slate-600 mb-1">Admin Address</label>
                <input
                  required
                  placeholder="G…"
                  value={params.adminAddress}
                  onChange={(e) => setParams((p) => ({ ...p, adminAddress: e.target.value }))}
                  className="w-full border border-slate-300 rounded-lg px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-indigo-500"
                />
                {wallet.publicKey && params.adminAddress !== wallet.publicKey && (
                  <button
                    type="button"
                    onClick={() =>
                      setParams((p) => ({ ...p, adminAddress: wallet.publicKey! }))
                    }
                    className="mt-1 text-xs text-indigo-600 hover:underline"
                  >
                    Use connected wallet address
                  </button>
                )}
              </div>

              <div className="flex items-center gap-3">
                <label className="text-xs font-semibold text-slate-600">Pause Vault</label>
                <button
                  type="button"
                  onClick={() => setParams((p) => ({ ...p, paused: !p.paused }))}
                  className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${
                    params.paused ? "bg-red-500" : "bg-slate-300"
                  }`}
                >
                  <span
                    className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform ${
                      params.paused ? "translate-x-4" : "translate-x-1"
                    }`}
                  />
                </button>
                <span className={`text-xs ${params.paused ? "text-red-600 font-medium" : "text-slate-400"}`}>
                  {params.paused ? "Paused" : "Active"}
                </span>
              </div>

              <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-xs text-amber-800">
                ⚠ This will sign a Soroban contract call via Freighter. Verify parameters before confirming.
              </div>

              <label className="flex items-center gap-2 text-sm text-slate-700 cursor-pointer">
                <input
                  type="checkbox"
                  checked={confirmed}
                  onChange={(e) => setConfirmed(e.target.checked)}
                  className="rounded"
                />
                I have reviewed the parameters and confirm submission
              </label>

              <button
                type="submit"
                disabled={!confirmed || signing || !wallet.connected}
                className="w-full bg-indigo-600 text-white py-2 rounded-lg text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
              >
                {signing
                  ? "Signing with Freighter…"
                  : !wallet.connected
                  ? "Connect wallet to sign"
                  : "Sign & Submit via Freighter"}
              </button>
            </form>
          </div>
        </div>
      )}

      {/* ── Audit Tab ──────────────────────────────────────────────────────────── */}
      {tab === "audit" && (
        <div className="space-y-4">
          {/* Summary Stats */}
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
            {[
              { label: "Total Events", value: stats.total, color: "text-slate-900" },
              { label: "Total Deposits", value: `${stats.deposits.toLocaleString()} USDC`, color: "text-emerald-700" },
              { label: "Total Withdrawals", value: `${stats.withdrawals.toLocaleString()} USDC`, color: "text-amber-700" },
              { label: "Yield Accrued", value: `${stats.yieldAcc.toLocaleString()} USDC`, color: "text-indigo-700" },
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

          {/* Filters */}
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
                  <path strokeLinecap="round" strokeLinejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                </svg>
                <input
                  id="audit-search"
                  placeholder="Search by tx hash, address, or block…"
                  value={search}
                  onChange={(e) => { setSearch(e.target.value); setPage(1); }}
                  className="w-full pl-9 pr-3 py-2 border border-slate-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                />
              </div>

              {/* Action Filter */}
              <select
                id="audit-action-filter"
                value={actionFilter}
                onChange={(e) => { setActionFilter(e.target.value as ActionFilter); setPage(1); }}
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
                onChange={(e) => { setDateFrom(e.target.value); setPage(1); }}
                className="border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                title="From date"
              />

              {/* Date To */}
              <input
                id="audit-date-to"
                type="date"
                value={dateTo}
                onChange={(e) => { setDateTo(e.target.value); setPage(1); }}
                className="border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                title="To date"
              />

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
                    <SortHeader label="Timestamp" k="timestamp" sortKey={sortKey} sortAsc={sortAsc} onSort={toggleSort} />
                    <SortHeader label="Address" k="address" sortKey={sortKey} sortAsc={sortAsc} onSort={toggleSort} />
                    <SortHeader label="Action" k="action" sortKey={sortKey} sortAsc={sortAsc} onSort={toggleSort} />
                    <SortHeader label="Volume" k="tokenVolume" sortKey={sortKey} sortAsc={sortAsc} onSort={toggleSort} />
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">Asset</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide whitespace-nowrap">Block</th>
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
                        <>
                          <tr
                            key={tx.id}
                            onClick={() => setExpandedRow(isExpanded ? null : tx.id)}
                            className="hover:bg-slate-50 transition-colors cursor-pointer"
                          >
                            <td className="px-4 py-3 font-mono text-xs text-slate-500 whitespace-nowrap">
                              <span title={tx.txHash}>{tx.txHash.slice(0, 10)}…{tx.txHash.slice(-6)}</span>
                            </td>
                            <td className="px-4 py-3 text-slate-600 whitespace-nowrap">
                              <div>{format(new Date(tx.timestamp), "MMM d, yyyy")}</div>
                              <div className="text-xs text-slate-400">{format(new Date(tx.timestamp), "HH:mm:ss")}</div>
                            </td>
                            <td className="px-4 py-3 font-mono text-xs text-slate-700 whitespace-nowrap">
                              <span title={tx.address}>{truncateKey(tx.address, 6, 6)}</span>
                            </td>
                            <td className="px-4 py-3">
                              <span className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-semibold ${meta.color}`}>
                                <span className={`w-1.5 h-1.5 rounded-full ${meta.dot}`} />
                                {meta.label}
                              </span>
                            </td>
                            <td className="px-4 py-3 font-medium text-slate-900 whitespace-nowrap">
                              {tx.tokenVolume > 0 ? tx.tokenVolume.toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 4 }) : "—"}
                            </td>
                            <td className="px-4 py-3 text-slate-500">{tx.asset}</td>
                            <td className="px-4 py-3 font-mono text-xs text-slate-500">
                              {tx.blockHeight?.toLocaleString() ?? "—"}
                            </td>
                          </tr>
                          {isExpanded && (
                            <tr key={`${tx.id}-expand`} className="bg-slate-50">
                              <td colSpan={7} className="px-6 py-4">
                                <div className="grid grid-cols-2 sm:grid-cols-3 gap-4 text-xs">
                                  <div>
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">Full Tx Hash</p>
                                    <p className="font-mono text-slate-700 break-all">{tx.txHash}</p>
                                  </div>
                                  <div>
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">Full Address</p>
                                    <p className="font-mono text-slate-700 break-all">{tx.address}</p>
                                  </div>
                                  <div>
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">Network Fee</p>
                                    <p className="text-slate-700">{tx.fee != null ? `${tx.fee} XLM` : "—"}</p>
                                  </div>
                                  <div>
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">Block Height</p>
                                    <p className="text-slate-700">{tx.blockHeight?.toLocaleString() ?? "—"}</p>
                                  </div>
                                  <div>
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">Timestamp (UTC)</p>
                                    <p className="text-slate-700">{tx.timestamp}</p>
                                  </div>
                                  <div>
                                    <p className="font-semibold text-slate-500 uppercase tracking-wide mb-1">Action</p>
                                    <p className="text-slate-700 capitalize">{tx.action.replace("_", " ")}</p>
                                  </div>
                                </div>
                              </td>
                            </tr>
                          )}
                        </>
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
    </div>
  );
}
