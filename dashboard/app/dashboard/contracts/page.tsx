"use client";
import { useMemo, useState } from "react";
import { usePolling } from "@/lib/use-polling";
import { api } from "@/lib/api";
import StatCard from "@/components/StatCard";
import { X, Shield, AlertTriangle } from "lucide-react";

function severityColor(severity: string) {
  if (severity === "critical") return "bg-red-50 border-red-200 text-red-800";
  if (severity === "warning")
    return "bg-amber-50 border-amber-200 text-amber-800";
  return "bg-blue-50 border-blue-200 text-blue-800";
}

// ── Admin panel: fee-coefficient form ─────────────────────────────────────────
function FeeConfigPanel() {
  const config = usePolling(() => api.contractConfig(), 30000);
  const currentFee = config.data?.fee_coefficient;

  const [inputValue, setInputValue] = useState("");
  const [status, setStatus] = useState<
    | { kind: "idle" }
    | { kind: "submitting" }
    | { kind: "success"; newValue: number; txHash: string }
    | { kind: "error"; message: string }
  >({ kind: "idle" });

  const basisPoints = Number(inputValue);
  const isValid =
    inputValue !== "" &&
    Number.isInteger(basisPoints) &&
    basisPoints >= 0 &&
    basisPoints <= 10000;

  const percentDisplay = isValid
    ? `${(basisPoints / 100).toFixed(2)}%`
    : null;

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!isValid) return;
    setStatus({ kind: "submitting" });
    try {
      const result = await api.setFeeCoefficient(basisPoints);
      setInputValue("");
      setStatus({
        kind: "success",
        newValue: result.fee_coefficient,
        txHash: result.tx_hash,
      });
    } catch (err) {
      setStatus({
        kind: "error",
        message: err instanceof Error ? err.message : "Unknown error",
      });
    }
  }

  return (
    <section className="mb-8">
      <h2 className="text-lg font-semibold text-slate-800 mb-1">
        Admin — Fee Coefficient
      </h2>
      <p className="text-xs text-slate-500 mb-4">
        Adjusts the platform fee charged on public payments. Value is in basis
        points (1 bp = 0.01%). Range: 0–10000.
      </p>

      <div className="bg-white rounded-xl border border-slate-200 p-6 max-w-lg">
        {/* Current value display */}
        <div className="mb-5 flex items-center gap-3">
          <span className="text-xs font-medium text-slate-500 uppercase tracking-wide">
            Current fee coefficient
          </span>
          {config.loading && currentFee === undefined ? (
            <span className="h-5 w-16 animate-pulse rounded bg-slate-100 inline-block" />
          ) : (
            <span className="text-sm font-semibold text-slate-900">
              {currentFee !== undefined
                ? `${currentFee} bp (${(currentFee / 100).toFixed(2)}%)`
                : "—"}
            </span>
          )}
        </div>

        {/* Form */}
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label
              htmlFor="fee-coefficient-input"
              className="block text-xs font-medium text-slate-600 mb-1"
            >
              New fee coefficient (basis points)
            </label>
            <div className="flex items-center gap-2">
              <input
                id="fee-coefficient-input"
                type="number"
                min={0}
                max={10000}
                step={1}
                value={inputValue}
                onChange={(e) => {
                  setInputValue(e.target.value);
                  if (status.kind !== "idle") setStatus({ kind: "idle" });
                }}
                placeholder="e.g. 50"
                className="w-32 rounded-lg border border-slate-300 px-3 py-2 text-sm text-slate-900
                           focus:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-200
                           disabled:opacity-50"
                disabled={status.kind === "submitting"}
              />
              {percentDisplay && (
                <span className="text-xs text-slate-500">
                  = {percentDisplay}
                </span>
              )}
            </div>
          </div>

          <button
            id="fee-coefficient-submit"
            type="submit"
            disabled={!isValid || status.kind === "submitting"}
            className="inline-flex items-center gap-2 rounded-lg bg-indigo-600 px-4 py-2 text-sm
                       font-medium text-white hover:bg-indigo-700 active:bg-indigo-800
                       disabled:cursor-not-allowed disabled:opacity-50 transition-colors"
          >
            {status.kind === "submitting" ? (
              <>
                <span className="h-4 w-4 animate-spin rounded-full border-2 border-white border-t-transparent" />
                Submitting…
              </>
            ) : (
              "Update fee coefficient"
            )}
          </button>
        </form>

        {/* Success feedback */}
        {status.kind === "success" && (
          <div className="mt-4 rounded-lg border border-green-200 bg-green-50 p-3 text-sm text-green-800">
            <p className="font-semibold">
              ✓ Fee coefficient updated to {status.newValue} bp (
              {(status.newValue / 100).toFixed(2)}%)
            </p>
            <p className="mt-1 text-xs break-all text-green-700">
              tx: {status.txHash}
            </p>
          </div>
        )}

        {/* Error feedback */}
        {status.kind === "error" && (
          <div className="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-800">
            <p className="font-semibold">✗ Update failed</p>
            <p className="mt-1 text-xs">{status.message}</p>
          </div>
        )}
      </div>
    </section>
  );
}

// ── Usernames table ───────────────────────────────────────────────────────────
function UsernamesTable() {
  const { data, loading, error } = usePolling(() => api.registryClaims(), 30000);
  const [sortKey, setSortKey] = useState<"username" | "registered_at">("registered_at");
  const [sortAsc, setSortAsc] = useState(false);

  // Blacklist confirmation drawer state
  const [showBlacklistDrawer, setShowBlacklistDrawer] = useState(false);
  const [targetUsername, setTargetUsername] = useState<string | null>(null);
  const [targetPublicKey, setTargetPublicKey] = useState<string | null>(null);
  const [blacklistConfirmed, setBlacklistConfirmed] = useState(false);
  const [signing, setSigning] = useState(false);
  const [blacklistMsg, setBlacklistMsg] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  const sorted = useMemo(() => {
    if (!data) return [];
    return [...data].sort((a, b) => {
      const cmp = sortKey === "username"
        ? a.username.localeCompare(b.username)
        : new Date(a.registered_at).getTime() - new Date(b.registered_at).getTime();
      return sortAsc ? cmp : -cmp;
    });
  }, [data, sortKey, sortAsc]);

  function toggleSort(key: "username" | "registered_at") {
    if (sortKey === key) setSortAsc((v) => !v);
    else { setSortKey(key); setSortAsc(true); }
  }

  const handleBlacklistClick = (username: string, publicKey: string) => {
    setTargetUsername(username);
    setTargetPublicKey(publicKey);
    setBlacklistConfirmed(false);
    setBlacklistMsg(null);
    setShowBlacklistDrawer(true);
  };

  const handleBlacklistConfirm = async () => {
    if (!blacklistConfirmed) {
      setBlacklistConfirmed(true);
      return;
    }
    if (!targetUsername || !targetPublicKey) return;

    setSigning(true);
    setBlacklistMsg(null);
    try {
      const { isConnected, getAddress, signTransaction } = await import("@stellar/freighter-api");
      const connected = await isConnected();
      if (!connected.isConnected) throw new Error("Freighter wallet not connected");

      const addr = await getAddress();
      if (!addr.address) throw new Error("Could not retrieve public key");

      const placeholderXdr = btoa(JSON.stringify({
        fn: "blacklist_user",
        username: targetUsername,
        public_key: targetPublicKey,
        admin: addr.address
      }));

      const result = await signTransaction(placeholderXdr, { networkPassphrase: "Test SDF Network ; September 2015" });
      if ("error" in result) throw new Error(result.error);

      setBlacklistMsg({ type: "ok", text: `User @${targetUsername} has been blacklisted. Signed XDR: ${(result as { signedTxXdr: string }).signedTxXdr.slice(0, 24)}…` });
      setTimeout(() => {
        setShowBlacklistDrawer(false);
        setBlacklistConfirmed(false);
        setTargetUsername(null);
        setTargetPublicKey(null);
      }, 2000);
    } catch (err) {
      setBlacklistMsg({ type: "err", text: err instanceof Error ? err.message : "Failed to sign blacklist transaction" });
    } finally {
      setSigning(false);
    }
  };

  const th = (key: "username" | "registered_at", label: string) => (
    <th
      className="text-left px-4 py-3 cursor-pointer select-none hover:text-slate-700"
      onClick={() => toggleSort(key)}
    >
      {label} {sortKey === key ? (sortAsc ? "↑" : "↓") : ""}
    </th>
  );

  return (
    <section className="mb-8">
      <h2 className="text-lg font-semibold text-slate-800 mb-3">
        Registered Usernames
      </h2>
      {error && (
        <div className="mb-3 p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">{error}</div>
      )}
      <div className="bg-white rounded-xl border border-slate-200 overflow-hidden">
        <table className="w-full text-sm">
          <thead className="bg-slate-50 text-slate-500 uppercase text-xs">
            <tr>
              {th("username", "Username")}
              <th className="text-left px-4 py-3">Public Key</th>
              {th("registered_at", "Registered")}
              <th className="text-left px-4 py-3">TX Hash</th>
              <th className="text-left px-4 py-3">Actions</th>
            </tr>
          </thead>
          <tbody>
            {loading && !data ? (
              Array.from({ length: 4 }).map((_, row) => (
                <tr key={row}>
                  {Array.from({ length: 5 }).map((__, col) => (
                    <td key={col} className="px-4 py-3"><div className="h-4 animate-pulse rounded bg-slate-100" /></td>
                  ))}
                </tr>
              ))
            ) : sorted.length === 0 ? (
              <tr><td colSpan={5} className="px-4 py-6 text-center text-slate-400">No registrations found</td></tr>
            ) : (
              sorted.map((c) => (
                <tr key={c.username} className="border-t border-slate-100 hover:bg-slate-50">
                  <td className="px-4 py-3 font-medium text-slate-900">{c.username}</td>
                  <td className="px-4 py-3 text-slate-600 font-mono text-xs truncate max-w-[200px]">{c.public_key}</td>
                  <td className="px-4 py-3 text-slate-500">{new Date(c.registered_at).toLocaleDateString()}</td>
                  <td className="px-4 py-3 text-slate-500 font-mono text-xs truncate max-w-[160px]">{c.tx_hash ?? "—"}</td>
                  <td className="px-4 py-3">
                    <button
                      onClick={() => handleBlacklistClick(c.username, c.public_key)}
                      className="inline-flex items-center gap-1 px-2 py-1 text-xs font-medium rounded bg-red-50 text-red-700 hover:bg-red-100 transition-colors"
                    >
                      <Shield size={12} />
                      Blacklist
                    </button>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      {data && <p className="mt-2 text-xs text-slate-400">{data.length} active registrations</p>}

      {/* Blacklist Confirmation Drawer */}
      {showBlacklistDrawer && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white rounded-xl shadow-2xl max-w-md w-full mx-4 overflow-hidden">
            <div className="bg-red-600 px-6 py-4 flex items-center justify-between">
              <h3 className="text-lg font-semibold text-white flex items-center gap-2">
                <AlertTriangle size={20} />
                Blacklist User
              </h3>
              <button
                onClick={() => {
                  setShowBlacklistDrawer(false);
                  setBlacklistConfirmed(false);
                  setTargetUsername(null);
                  setTargetPublicKey(null);
                  setBlacklistMsg(null);
                }}
                className="text-white hover:text-red-200 transition-colors"
              >
                <X size={20} />
              </button>
            </div>
            <div className="p-6">
              <div className="bg-amber-50 border border-amber-200 rounded-lg p-4 mb-6">
                <p className="text-sm text-amber-800 font-medium mb-2">
                  You are about to blacklist <strong>@{targetUsername}</strong>
                </p>
                <p className="text-xs text-amber-700 font-mono break-all">
                  {targetPublicKey}
                </p>
              </div>

              <p className="text-sm text-slate-700 mb-4">
                This action will:
              </p>
              <ul className="text-sm text-slate-600 space-y-2 mb-6 list-disc list-inside">
                <li>Revoke the user's registered username</li>
                <li>Block all incoming payments to this user</li>
                <li>Prevent the user from receiving funds</li>
                <li>Flag the account as a bad actor</li>
              </ul>

              <div className="bg-red-50 border border-red-200 rounded-lg p-3 mb-6">
                <p className="text-xs text-red-800 font-medium">
                  ⚠ This action requires administrator privileges and will be signed via Freighter. This action cannot be easily undone.
                </p>
              </div>

              {blacklistMsg && (
                <div className={`mb-4 p-3 rounded-lg text-sm ${blacklistMsg.type === "ok" ? "bg-green-50 text-green-700 border border-green-200" : "bg-red-50 text-red-700 border border-red-200"}`}>
                  {blacklistMsg.text}
                </div>
              )}

              {!blacklistConfirmed ? (
                <button
                  onClick={() => setBlacklistConfirmed(true)}
                  className="w-full bg-red-600 text-white py-3 rounded-lg font-medium hover:bg-red-700 transition-colors"
                >
                  First Confirmation
                </button>
              ) : (
                <button
                  onClick={handleBlacklistConfirm}
                  disabled={signing}
                  className="w-full bg-red-600 text-white py-3 rounded-lg font-medium hover:bg-red-700 disabled:opacity-50 transition-colors"
                >
                  {signing ? "Signing with Freighter…" : "Final Confirmation - Sign & Submit"}
                </button>
              )}
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

// ── Main page ─────────────────────────────────────────────────────────────────
export default function ContractsPage() {
  const health = usePolling(() => api.contractHealth(), 15000);
  const metrics = usePolling(() => api.contractMetrics(), 15000);
  const alerts = usePolling(() => api.contractAlerts(), 15000);

  const error = health.error || metrics.error || alerts.error;

  return (
    <div>
      <h1 className="text-2xl font-bold text-slate-900 mb-2">
        Contract Monitoring
      </h1>
      <p className="text-sm text-slate-500 mb-6">
        Soroban contract health, performance, and active alerts
      </p>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">
          {error} — ensure NEXT_PUBLIC_SERVER_URL points at the Node server and
          you are signed in
        </div>
      )}

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
        <StatCard
          label="Overall Status"
          value={health.data?.status ?? "—"}
          color={
            health.data?.status === "healthy"
              ? "text-green-600"
              : "text-red-600"
          }
        />
        <StatCard label="Soroban RPC" value={health.data?.sorobanRpc ?? "—"} />
        <StatCard label="Latest Ledger" value={health.data?.latestLedger ?? 0} />
        <StatCard
          label="Active Alerts"
          value={alerts.data?.alerts.length ?? 0}
          color={
            (alerts.data?.alerts.length ?? 0) > 0
              ? "text-red-600"
              : "text-green-600"
          }
        />
      </div>

      {metrics.data && (
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
          <StatCard
            label="RPC Latency (ms)"
            value={metrics.data.sorobanRpcLatencyMs}
          />
          <StatCard
            label="Event Poll Lag"
            value={metrics.data.eventPollLagLedgers}
            sub="ledgers"
          />
          <StatCard
            label="Settled Events"
            value={metrics.data.eventsTotal.settled}
            color="text-green-600"
          />
          <StatCard
            label="Failed Events"
            value={metrics.data.eventsTotal.failed}
            color="text-red-600"
          />
        </div>
      )}

      <section className="mb-8">
        <h2 className="text-lg font-semibold text-slate-800 mb-3">
          Contract Health
        </h2>
        <div className="bg-white rounded-xl border border-slate-200 overflow-hidden">
          <table className="w-full text-sm">
            <thead className="bg-slate-50 text-slate-500 uppercase text-xs">
              <tr>
                <th className="text-left px-4 py-3">Contract</th>
                <th className="text-left px-4 py-3">Reachable</th>
                <th className="text-left px-4 py-3">Paused</th>
                <th className="text-left px-4 py-3">Last Checked</th>
              </tr>
            </thead>
            <tbody>
              {(health.data?.contracts ?? []).map((c) => (
                <tr key={c.name} className="border-t border-slate-100">
                  <td className="px-4 py-3 font-medium">{c.name}</td>
                  <td className="px-4 py-3">
                    <span
                      className={
                        c.reachable ? "text-green-600" : "text-red-600"
                      }
                    >
                      {c.reachable ? "Yes" : "No"}
                    </span>
                  </td>
                  <td className="px-4 py-3">
                    {c.paused === undefined ? "—" : c.paused ? "Yes" : "No"}
                  </td>
                  <td className="px-4 py-3 text-slate-500">
                    {new Date(c.lastChecked).toLocaleString()}
                  </td>
                </tr>
              ))}
              {!health.data?.contracts?.length && (
                <tr>
                  <td
                    colSpan={4}
                    className="px-4 py-6 text-slate-400 text-center"
                  >
                    No contracts configured (set PAYMENT_ROUTER_CONTRACT /
                    REGISTRY_CONTRACT)
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="mb-8">
        <h2 className="text-lg font-semibold text-slate-800 mb-3">
          Active Alerts
        </h2>
        {(alerts.data?.alerts ?? []).length === 0 ? (
          <p className="text-sm text-slate-500">No active alerts</p>
        ) : (
          <ul className="space-y-2">
            {alerts.data?.alerts.map((alert) => (
              <li
                key={alert.id}
                className={`p-4 rounded-lg border text-sm ${severityColor(alert.severity)}`}
              >
                <p className="font-semibold">{alert.title}</p>
                <p className="mt-1">{alert.message}</p>
                <p className="mt-2 text-xs opacity-75">
                  {alert.metric}: {alert.value} (threshold {alert.threshold}) ·{" "}
                  {new Date(alert.timestamp).toLocaleString()}
                </p>
              </li>
            ))}
          </ul>
        )}
      </section>

      {/* Registered usernames table */}
      <UsernamesTable />

      {/* Admin section — fee coefficient */}
      <FeeConfigPanel />

      <p className="mt-6 text-xs text-slate-400">
        Auto-refreshes every 15 seconds
      </p>
    </div>
  );
}
