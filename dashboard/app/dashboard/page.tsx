"use client";

import { useCallback } from "react";
import { format } from "date-fns";
import StatCard from "@/components/StatCard";
import { api, type AdminAuditLog } from "@/lib/api";
import { usePolling } from "@/lib/use-polling";

function fmtUsdc(value: number): string {
  return (
    value.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }) + " USDC"
  );
}

function maskEmail(email: string): string {
  const [local, domain] = email.split("@");
  if (!domain) return "***";
  const masked = local.length > 1 ? local[0] + "***" : "*";
  return `${masked}@${domain}`;
}

function maskPhone(phone: string): string {
  if (phone.length <= 4) return "***";
  return phone.slice(0, 3) + "***" + phone.slice(-2);
}

function downloadCSV(rows: string[], filename: string): void {
  const blob = new Blob([rows.join("\n")], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}

export default function OverviewPage() {
  const { data: feedData, loading: feedLoading, error: feedError } = usePolling(
    () => api.socialFeed(),
    15000,
  );

  const { data: yieldData, loading: yieldLoading, error: yieldError } = usePolling(
    () => api.yieldStats(),
    30000,
  );

  const { data: registryData, loading: registryLoading, error: registryError } = usePolling(
    () => api.registryStats(),
    30000,
  );

  // #797 — Admin audit log, descending by timestamp, refreshed every 60 s
  const { data: logsData, loading: logsLoading, error: logsError } = usePolling(
    () => api.adminLogs(50, 0),
    60_000,
  );

  const likes = feedData?.reduce((total, feed) => total + feed.likes_count, 0) ?? 0;
  const comments = feedData?.reduce((total, feed) => total + feed.comments_count, 0) ?? 0;
  const activeFeeds = feedData?.length ?? 0;

  const tvl = yieldData?.total_value_locked ?? 0;
  const yieldDistributed = yieldData?.total_yield_distributed ?? 0;
  const apy = yieldData?.apy ?? 0;

  const handleExportUsers = useCallback(async () => {
    try {
      const links = await api.identityLinks();
      const headers = ["User ID", "Privy DID", "Stellar Address", "Display Name", "Email", "Phone", "Status", "Linked At"];
      const csvRows = [headers.join(",")];
      for (const link of links.links) {
        const email = link.email ? maskEmail(link.email) : "";
        const phone = link.display_name ? maskPhone(link.display_name) : "";
        csvRows.push([
          link.user_id,
          link.privy_did,
          link.stellar_address,
          link.display_name ?? "",
          email,
          phone,
          link.status,
          link.linked_at,
        ].map((v) => `"${v}"`).join(","));
      }
      downloadCSV(csvRows, `privy-users-export-${new Date().toISOString().slice(0, 10)}.csv`);
    } catch {
      // silently fail
    }
  }, []);

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-slate-900">Social Overview</h1>
          <p className="mt-1 text-sm text-slate-500">
            Live engagement across recent payment feeds.
          </p>
        </div>
        <button
          onClick={handleExportUsers}
          className="inline-flex items-center gap-2 rounded-lg border border-slate-300 bg-white px-4 py-2 text-sm font-medium text-slate-700 shadow-sm hover:bg-slate-50 transition-colors"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" x2="12" y1="15" y2="3"/></svg>
          Export Users (CSV)
        </button>
      </div>

      {/* Social Overview */}
      {feedError && (
        <div className="mb-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          {feedError} — showing the most recently loaded values
        </div>
      )}

      {feedLoading && !feedData ? (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          {Array.from({ length: 3 }).map((_, index) => (
            <div
              key={index}
              className="h-28 animate-pulse rounded-xl border border-slate-200 bg-white"
            />
          ))}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <StatCard
            label="Total Likes"
            value={likes}
            sub="Across recent social payments"
            color="text-pink-600"
          />
          <StatCard
            label="Total Comments"
            value={comments}
            sub="Conversation on payment feeds"
            color="text-indigo-600"
          />
          <StatCard
            label="Active Social Feeds"
            value={activeFeeds}
            sub="Recent public feeds"
            color="text-emerald-600"
          />
        </div>
      )}

      {/* Username Registry */}
      <div className="mt-10 mb-6">
        <h2 className="text-lg font-semibold text-slate-900">Username Registry</h2>
        <p className="mt-1 text-sm text-slate-500">
          Registration metrics and weekly growth indicators.
        </p>
      </div>

      {registryError && (
        <div className="mb-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          {registryError} — showing the most recently loaded values
        </div>
      )}

      {registryLoading && !registryData ? (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          {Array.from({ length: 3 }).map((_, index) => (
            <div
              key={index}
              className="h-28 animate-pulse rounded-xl border border-slate-200 bg-white"
            />
          ))}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <StatCard
            label="Total Registered"
            value={registryData?.total_usernames ?? 0}
            sub="Unique usernames on-chain"
            color="text-indigo-600"
          />
          <StatCard
            label="Weekly Growth"
            value={registryData?.weekly_growth ?? 0}
            sub="New registrations this week"
            color="text-emerald-600"
          />
          <StatCard
            label="Active Registrations"
            value={registryData?.active_registrations ?? 0}
            sub="Currently active claims"
            color="text-amber-600"
          />
        </div>
      )}

      {/* Yield Metrics */}
      <div className="mt-10 mb-6">
        <h2 className="text-lg font-semibold text-slate-900">Yield Vault</h2>
        <p className="mt-1 text-sm text-slate-500">
          Aggregate metrics from the on-chain yield vault.
        </p>
      </div>

      {yieldError && (
        <div className="mb-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          {yieldError} — showing the most recently loaded values
        </div>
      )}

      {yieldLoading && !yieldData ? (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          {Array.from({ length: 3 }).map((_, index) => (
            <div
              key={index}
              className="h-28 animate-pulse rounded-xl border border-slate-200 bg-white"
            />
          ))}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <StatCard
            label="Total Value Locked"
            value={fmtUsdc(tvl)}
            sub="Active deposits in the vault"
            color="text-indigo-600"
          />
          <StatCard
            label="Total Yield Distributed"
            value={fmtUsdc(yieldDistributed)}
            sub="Claimed by depositors to date"
            color="text-emerald-600"
          />
          <StatCard
            label="Current APY"
            value={`${apy.toFixed(1)}%`}
            sub="Annualised yield rate"
            color="text-amber-600"
          />
        </div>
      )}

      <p className="mt-4 text-xs text-slate-400">
        Social stats refresh every 15 s · Vault stats refresh every 30 s
      </p>

      {/* ── #797 Admin Audit Log ─────────────────────────────────────────── */}
      <div className="mt-10 mb-6">
        <h2 className="text-lg font-semibold text-slate-900">Admin Audit Log</h2>
        <p className="mt-1 text-sm text-slate-500">
          Configuration changes and admin actions, newest first.
        </p>
      </div>

      {logsError && (
        <div className="mb-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          {logsError} — could not load audit log
        </div>
      )}

      <div className="overflow-hidden rounded-xl border border-slate-200 bg-white shadow-sm">
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="border-b border-slate-200 bg-slate-50">
              <tr>
                {["Timestamp", "Admin ID", "Action", "Details", "IP Address"].map(
                  (heading) => (
                    <th
                      key={heading}
                      className="px-4 py-3 text-left text-xs font-semibold uppercase tracking-wide text-slate-500"
                    >
                      {heading}
                    </th>
                  ),
                )}
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100">
              {logsLoading && !logsData ? (
                Array.from({ length: 5 }).map((_, row) => (
                  <tr key={row}>
                    {Array.from({ length: 5 }).map((__, col) => (
                      <td key={col} className="px-4 py-3">
                        <div className="h-4 animate-pulse rounded bg-slate-100" />
                      </td>
                    ))}
                  </tr>
                ))
              ) : !logsData?.logs?.length ? (
                <tr>
                  <td
                    colSpan={5}
                    className="px-4 py-10 text-center text-slate-400"
                  >
                    No audit log entries found
                  </td>
                </tr>
              ) : (
                // Already ordered descending by server; display as-is
                logsData.logs.map((log: AdminAuditLog) => (
                  <tr key={log.id} className="hover:bg-slate-50 transition-colors">
                    <td className="whitespace-nowrap px-4 py-3 text-slate-600">
                      {format(new Date(log.timestamp), "MMM d, yyyy HH:mm:ss")}
                    </td>
                    <td className="whitespace-nowrap px-4 py-3 font-mono text-xs text-slate-700">
                      {log.admin_id}
                    </td>
                    <td className="px-4 py-3">
                      <span className="inline-flex rounded-md bg-indigo-50 px-2 py-0.5 text-xs font-medium text-indigo-700 ring-1 ring-inset ring-indigo-600/20">
                        {log.action}
                      </span>
                    </td>
                    <td className="max-w-xs px-4 py-3 text-slate-600">
                      <span className="line-clamp-2 text-xs">
                        {log.details ?? "—"}
                      </span>
                    </td>
                    <td className="whitespace-nowrap px-4 py-3 font-mono text-xs text-slate-500">
                      {log.ip_address ?? "—"}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>

      <p className="mt-2 text-xs text-slate-400">
        Audit log refreshes every 60 s
      </p>
    </div>
  );
}
