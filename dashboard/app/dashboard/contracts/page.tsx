"use client";
import { usePolling } from "@/lib/use-polling";
import { api } from "@/lib/api";
import StatCard from "@/components/StatCard";

function severityColor(severity: string) {
  if (severity === "critical") return "bg-red-50 border-red-200 text-red-800";
  if (severity === "warning") return "bg-amber-50 border-amber-200 text-amber-800";
  return "bg-blue-50 border-blue-200 text-blue-800";
}

export default function ContractsPage() {
  const health = usePolling(() => api.contractHealth(), 15000);
  const metrics = usePolling(() => api.contractMetrics(), 15000);
  const alerts = usePolling(() => api.contractAlerts(), 15000);

  const error = health.error || metrics.error || alerts.error;

  return (
    <div>
      <h1 className="text-2xl font-bold text-slate-900 mb-2">Contract Monitoring</h1>
      <p className="text-sm text-slate-500 mb-6">
        Soroban contract health, performance, and active alerts
      </p>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">
          {error} — ensure NEXT_PUBLIC_SERVER_URL points at the Node server and you are signed in
        </div>
      )}

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
        <StatCard
          label="Overall Status"
          value={health.data?.status ?? "—"}
          color={health.data?.status === "healthy" ? "text-green-600" : "text-red-600"}
        />
        <StatCard label="Soroban RPC" value={health.data?.sorobanRpc ?? "—"} />
        <StatCard label="Latest Ledger" value={health.data?.latestLedger ?? 0} />
        <StatCard
          label="Active Alerts"
          value={alerts.data?.alerts.length ?? 0}
          color={(alerts.data?.alerts.length ?? 0) > 0 ? "text-red-600" : "text-green-600"}
        />
      </div>

      {metrics.data && (
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
          <StatCard label="RPC Latency (ms)" value={metrics.data.sorobanRpcLatencyMs} />
          <StatCard label="Event Poll Lag" value={metrics.data.eventPollLagLedgers} sub="ledgers" />
          <StatCard label="Settled Events" value={metrics.data.eventsTotal.settled} color="text-green-600" />
          <StatCard label="Failed Events" value={metrics.data.eventsTotal.failed} color="text-red-600" />
        </div>
      )}

      <section className="mb-8">
        <h2 className="text-lg font-semibold text-slate-800 mb-3">Contract Health</h2>
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
                    <span className={c.reachable ? "text-green-600" : "text-red-600"}>
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
                  <td colSpan={4} className="px-4 py-6 text-slate-400 text-center">
                    No contracts configured (set PAYMENT_ROUTER_CONTRACT / REGISTRY_CONTRACT)
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section>
        <h2 className="text-lg font-semibold text-slate-800 mb-3">Active Alerts</h2>
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

      <p className="mt-6 text-xs text-slate-400">Auto-refreshes every 15 seconds</p>
    </div>
  );
}
