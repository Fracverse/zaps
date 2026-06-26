"use client";
import { useState } from "react";
import { usePolling } from "@/lib/use-polling";
import { api } from "@/lib/api";
import StatCard from "@/components/StatCard";

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

      {/* Admin section — fee coefficient */}
      <FeeConfigPanel />

      <p className="mt-6 text-xs text-slate-400">
        Auto-refreshes every 15 seconds
      </p>
    </div>
  );
}
