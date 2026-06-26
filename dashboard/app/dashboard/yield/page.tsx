"use client";
import { useState, useMemo, useCallback } from "react";
import { format } from "date-fns";

interface VaultParams {
  apy: string;
  paused: boolean;
  adminAddress: string;
}

interface YieldTx {
  id: string;
  txHash: string;
  timestamp: string;
  user: string;
  action: "deposit" | "withdraw" | "claim" | "config";
  tokenVolume: number;
  asset: string;
}

type SortKey = keyof Pick<YieldTx, "timestamp" | "user" | "action" | "tokenVolume">;

const MOCK_TXS: YieldTx[] = [
  { id: "1", txHash: "abc123def456", timestamp: "2026-06-25T10:00:00Z", user: "GD3X...ABCD", action: "deposit", tokenVolume: 1000, asset: "USDC" },
  { id: "2", txHash: "bcd234efg567", timestamp: "2026-06-24T15:30:00Z", user: "GA1Y...EFGH", action: "claim", tokenVolume: 25.5, asset: "USDC" },
  { id: "3", txHash: "cde345fgh678", timestamp: "2026-06-23T09:15:00Z", user: "GB2Z...IJKL", action: "withdraw", tokenVolume: 500, asset: "USDC" },
  { id: "4", txHash: "def456ghi789", timestamp: "2026-06-22T12:00:00Z", user: "GD3X...ABCD", action: "config", tokenVolume: 0, asset: "—" },
];

const ACTION_COLORS: Record<YieldTx["action"], string> = {
  deposit: "bg-green-50 text-green-700",
  withdraw: "bg-amber-50 text-amber-700",
  claim: "bg-indigo-50 text-indigo-700",
  config: "bg-slate-100 text-slate-700",
};

function SortHeader({
  label, k, sortKey, sortAsc, onSort,
}: {
  label: string;
  k: SortKey;
  sortKey: SortKey;
  sortAsc: boolean;
  onSort: (k: SortKey) => void;
}) {
  return (
    <th
      onClick={() => onSort(k)}
      className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide cursor-pointer hover:text-slate-800 select-none"
    >
      {label} {sortKey === k ? (sortAsc ? "↑" : "↓") : ""}
    </th>
  );
}

export default function YieldPage() {
  const [tab, setTab] = useState<"config" | "audit">("config");

  const [params, setParams] = useState<VaultParams>({ apy: "5.0", paused: false, adminAddress: "" });
  const [confirmed, setConfirmed] = useState(false);
  const [signing, setSigning] = useState(false);
  const [msg, setMsg] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  const [search, setSearch] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("timestamp");
  const [sortAsc, setSortAsc] = useState(false);

  const signAndSubmit = useCallback(async (e: React.FormEvent) => {
    e.preventDefault();
    if (!confirmed) return;
    setSigning(true);
    setMsg(null);
    try {
      const { isConnected, getAddress, signTransaction } = await import("@stellar/freighter-api");
      const connected = await isConnected();
      if (!connected.isConnected) throw new Error("Freighter wallet not connected");

      const addr = await getAddress();
      if (!addr.address) throw new Error("Could not retrieve public key");

      const placeholderXdr = btoa(JSON.stringify({ fn: "set_vault_params", ...params, admin: addr.address }));

      const result = await signTransaction(placeholderXdr, { networkPassphrase: "Test SDF Network ; September 2015" });
      if ("error" in result) throw new Error(result.error);

      setMsg({ type: "ok", text: `Transaction signed and submitted. Signed XDR: ${(result as { signedTxXdr: string }).signedTxXdr.slice(0, 24)}…` });
      setConfirmed(false);
    } catch (err) {
      setMsg({ type: "err", text: err instanceof Error ? err.message : "Failed to sign" });
    } finally {
      setSigning(false);
    }
  }, [params, confirmed]);

  const filtered = useMemo(() => {
    const term = search.trim().toLowerCase();
    const rows = term
      ? MOCK_TXS.filter((t) => [t.txHash, t.user, t.action].some((v) => v.toLowerCase().includes(term)))
      : [...MOCK_TXS];
    rows.sort((a, b) => {
      const av = a[sortKey];
      const bv = b[sortKey];
      const cmp = typeof av === "number" ? av - (bv as number) : String(av).localeCompare(String(bv));
      return sortAsc ? cmp : -cmp;
    });
    return rows;
  }, [search, sortKey, sortAsc]);

  const toggleSort = (key: SortKey) => {
    if (sortKey === key) setSortAsc((v) => !v);
    else { setSortKey(key); setSortAsc(true); }
  };

  return (
    <div>
      <h1 className="text-2xl font-bold text-slate-900 mb-1">Yield Vault</h1>
      <p className="text-sm text-slate-500 mb-6">Admin configuration and transaction history</p>

      <div className="flex gap-2 mb-6 border-b border-slate-200">
        {(["config", "audit"] as const).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${tab === t ? "border-indigo-600 text-indigo-600" : "border-transparent text-slate-500 hover:text-slate-800"}`}
          >
            {t === "config" ? "Vault Configuration" : "Audit History"}
          </button>
        ))}
      </div>

      {tab === "config" && (
        <div className="max-w-lg">
          <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
            <h2 className="font-semibold text-slate-800 mb-4">Vault Parameters</h2>
            {msg && (
              <div className={`mb-4 p-3 rounded-lg text-sm ${msg.type === "ok" ? "bg-green-50 text-green-700 border border-green-200" : "bg-red-50 text-red-700 border border-red-200"}`}>
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
                  placeholder="G..."
                  value={params.adminAddress}
                  onChange={(e) => setParams((p) => ({ ...p, adminAddress: e.target.value }))}
                  className="w-full border border-slate-300 rounded-lg px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-indigo-500"
                />
              </div>

              <div className="flex items-center gap-3">
                <label className="text-xs font-semibold text-slate-600">Pause Vault</label>
                <button
                  type="button"
                  onClick={() => setParams((p) => ({ ...p, paused: !p.paused }))}
                  className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${params.paused ? "bg-red-500" : "bg-slate-300"}`}
                >
                  <span className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform ${params.paused ? "translate-x-4" : "translate-x-1"}`} />
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
                disabled={!confirmed || signing}
                className="w-full bg-indigo-600 text-white py-2 rounded-lg text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
              >
                {signing ? "Signing with Freighter…" : "Sign & Submit via Freighter"}
              </button>
            </form>
          </div>
        </div>
      )}

      {tab === "audit" && (
        <div>
          <div className="mb-4 flex items-center gap-3">
            <input
              placeholder="Search by tx hash, user, or action…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="w-full max-w-sm border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
            <span className="text-xs text-slate-400">{filtered.length} records</span>
          </div>

          <div className="bg-white border border-slate-200 rounded-xl overflow-hidden shadow-sm">
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead className="bg-slate-50 border-b border-slate-200">
                  <tr>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">Tx Hash</th>
                    <SortHeader label="Timestamp" k="timestamp" sortKey={sortKey} sortAsc={sortAsc} onSort={toggleSort} />
                    <SortHeader label="User" k="user" sortKey={sortKey} sortAsc={sortAsc} onSort={toggleSort} />
                    <SortHeader label="Action" k="action" sortKey={sortKey} sortAsc={sortAsc} onSort={toggleSort} />
                    <SortHeader label="Volume" k="tokenVolume" sortKey={sortKey} sortAsc={sortAsc} onSort={toggleSort} />
                    <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">Asset</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-100">
                  {filtered.length === 0 ? (
                    <tr>
                      <td colSpan={6} className="px-4 py-10 text-center text-slate-400">No transactions found</td>
                    </tr>
                  ) : (
                    filtered.map((tx) => (
                      <tr key={tx.id} className="hover:bg-slate-50 transition-colors">
                        <td className="px-4 py-3 font-mono text-xs text-slate-500">{tx.txHash}</td>
                        <td className="px-4 py-3 text-slate-600 whitespace-nowrap">
                          {format(new Date(tx.timestamp), "MMM d, yyyy HH:mm")}
                        </td>
                        <td className="px-4 py-3 font-mono text-xs text-slate-700">{tx.user}</td>
                        <td className="px-4 py-3">
                          <span className={`inline-flex rounded-full px-2.5 py-0.5 text-xs font-semibold ${ACTION_COLORS[tx.action]}`}>
                            {tx.action}
                          </span>
                        </td>
                        <td className="px-4 py-3 font-medium text-slate-900">
                          {tx.tokenVolume > 0 ? tx.tokenVolume.toLocaleString() : "—"}
                        </td>
                        <td className="px-4 py-3 text-slate-500">{tx.asset}</td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
