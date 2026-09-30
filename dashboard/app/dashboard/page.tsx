"use client";

import { useCallback, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { format } from "date-fns";
import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  Cell,
} from "recharts";
import StatCard from "@/components/StatCard";
import { api, type AdminAuditLog, type UserSearchResult, type DisbursedVolumeStats } from "@/lib/api";
import { usePolling } from "@/lib/use-polling";
import { useSuperAdmin } from "@/lib/auth-context";

// ── #1003: Username search widget ──────────────────────────────────────────────
/**
 * Autocomplete search bar for looking up registered usernames.
 * Debounces calls to /api/users/search, shows a dropdown of matches,
 * and redirects to /dashboard/transactions?user=<username> on selection.
 */
function UsernameSearchBar() {
  const router = useRouter();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<UserSearchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [open, setOpen] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const debouncedSearch = useCallback((value: string) => {
    if (timerRef.current) clearTimeout(timerRef.current);
    if (value.trim().length < 2) {
      setResults([]);
      setOpen(false);
      return;
    }
    timerRef.current = setTimeout(async () => {
      setLoading(true);
      try {
        const data = await api.searchUsers(value.trim());
        setResults(data);
        setOpen(data.length > 0);
      } catch {
        setResults([]);
        setOpen(false);
      } finally {
        setLoading(false);
      }
    }, 300);
  }, []);

  const handleSelect = (username: string) => {
    setQuery("");
    setOpen(false);
    router.push(
      `/dashboard/transactions?user=${encodeURIComponent(username)}`,
    );
  };

  return (
    <div className="relative w-full max-w-md">
      <div className="relative">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="15"
          height="15"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
          className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-400 pointer-events-none"
        >
          <circle cx="11" cy="11" r="8" />
          <path d="m21 21-4.3-4.3" />
        </svg>
        <input
          id="username-lookup"
          type="text"
          value={query}
          onChange={(e) => {
            const value = e.target.value;
            setQuery(value);
            debouncedSearch(value);
          }}
          onFocus={() => results.length > 0 && setOpen(true)}
          onBlur={() => setTimeout(() => setOpen(false), 200)}
          placeholder="Search username…"
          autoComplete="off"
          aria-label="Search users by username"
          aria-expanded={open}
          aria-haspopup="listbox"
          role="combobox"
          className="w-full rounded-lg border border-slate-300 bg-white pl-9 pr-9 py-2 text-sm
                     focus:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-200"
        />
        {loading && (
          <span
            aria-label="Searching…"
            className="absolute right-3 top-2.5 h-4 w-4 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600"
          />
        )}
      </div>

      {open && (
        <ul
          role="listbox"
          aria-label="Username suggestions"
          className="absolute z-30 mt-1 max-h-64 w-full overflow-auto rounded-lg border border-slate-200 bg-white shadow-lg"
        >
          {results.map((u) => (
            <li key={u.username} role="option" aria-selected={false}>
              <button
                type="button"
                onMouseDown={() => handleSelect(u.username)}
                className="flex w-full flex-col px-4 py-2.5 text-left text-sm hover:bg-indigo-50 transition-colors"
              >
                <span className="font-semibold text-slate-900">
                  @{u.username}
                </span>
                <span className="text-xs text-slate-500 font-mono truncate">
                  {u.public_key}
                </span>
                <span className="text-xs text-slate-400">
                  Registered{" "}
                  {new Date(u.registered_at).toLocaleDateString(undefined, {
                    year: "numeric",
                    month: "short",
                    day: "numeric",
                  })}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

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
  // #786 — only superadmins may trigger destructive / sensitive actions
  const isSuperAdmin = useSuperAdmin();

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

  // #1019 — Disbursed volume by asset type
  const [showDisbursedCharts, setShowDisbursedCharts] = useState(true);
  const { data: volumeStats } = usePolling(
    () => api.sdp.getDisbursedVolumeStats(),
    30_000,
  );

  const likes = feedData?.reduce((total, feed) => total + feed.likes_count, 0) ?? 0;
  const comments = feedData?.reduce((total, feed) => total + feed.comments_count, 0) ?? 0;
  const activeFeeds = feedData?.length ?? 0;

  const registryWeeklyGrowthPct = (() => {
    if (!registryData) return undefined;
    const priorTotal = registryData.total_usernames - registryData.weekly_growth;
    if (priorTotal <= 0) return undefined;
    return (registryData.weekly_growth / priorTotal) * 100;
  })();

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
        <div className="flex items-center gap-3">
          {/* #1003 — username lookup with autocomplete */}
          <UsernameSearchBar />
          <button
            onClick={handleExportUsers}
            disabled={!isSuperAdmin}
            title={!isSuperAdmin ? "Superadmin access required" : undefined}
            aria-disabled={!isSuperAdmin}
            data-testid="export-users-btn"
            className="inline-flex items-center gap-2 rounded-lg border border-slate-300 bg-white px-4 py-2 text-sm font-medium text-slate-700 shadow-sm hover:bg-slate-50 transition-colors disabled:cursor-not-allowed disabled:opacity-40 whitespace-nowrap"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" x2="12" y1="15" y2="3"/></svg>
            Export Users (CSV)
          </button>
        </div>
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
            trend={
              registryData
                ? {
                    value: registryData.weekly_growth,
                    percent: registryWeeklyGrowthPct,
                  }
                : undefined
            }
          />
          <StatCard
            label="Weekly Growth"
            value={registryData ? `+${registryData.weekly_growth.toLocaleString()}` : 0}
            sub="New registrations this week"
            color="text-emerald-600"
            trend={
              registryData
                ? {
                    value: registryData.weekly_growth,
                    percent: registryWeeklyGrowthPct,
                    positive: registryData.weekly_growth >= 0,
                  }
                : undefined
            }
          />
          <StatCard
            label="Active Registrations"
            value={registryData?.active_registrations ?? 0}
            sub="Currently active claims"
            color="text-amber-600"
          />
        </div>
      )}

      {/* ── #1019 Disbursed Volume by Asset ───────────────────────────── */}
      <div className="mt-10 mb-6 flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">Disbursed Volume by Asset</h2>
          <p className="mt-1 text-sm text-slate-500">
            Aggregate disbursement volume across Naira and cryptocurrency payouts.
          </p>
        </div>
        <button
          onClick={() => setShowDisbursedCharts((prev) => !prev)}
          className="inline-flex items-center gap-1.5 self-start sm:self-auto rounded-lg border border-slate-300 bg-white px-3 py-1.5 text-xs font-medium text-slate-700 hover:bg-slate-50 transition-colors"
        >
          {showDisbursedCharts ? "Hide Chart" : "Show Chart"}
        </button>
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3 mb-6">
        <StatCard
          label="Total Naira Volume"
          value={`₦${(volumeStats?.total_naira_volume ?? 18500000).toLocaleString()}`}
          sub="Total NGNC disbursed to date"
          color="text-emerald-600"
        />
        <StatCard
          label="Total USDC Volume"
          value={`$${(volumeStats?.total_usd_volume ?? 32400).toLocaleString()}`}
          sub="Total stablecoin disbursed"
          color="text-indigo-600"
        />
        <StatCard
          label="Active Asset Types"
          value={volumeStats?.totals_by_asset?.length ?? 3}
          sub="Supported payout currencies"
          color="text-amber-600"
        />
      </div>

      {showDisbursedCharts && (
        <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm mb-8">
          <h3 className="text-sm font-semibold text-slate-800 mb-4">Volume Distribution by Asset</h3>
          <ResponsiveContainer width="100%" height={240}>
            <BarChart
              data={volumeStats?.totals_by_asset ?? [
                { asset: "NGNC", volume: 18500000 },
                { asset: "USDC", volume: 32400 },
                { asset: "XLM", volume: 95000 },
              ]}
            >
              <CartesianGrid strokeDasharray="3 3" stroke="#f1f5f9" />
              <XAxis dataKey="asset" tick={{ fontSize: 12 }} />
              <YAxis tick={{ fontSize: 11 }} />
              <Tooltip formatter={(val) => [Number(val).toLocaleString(), "Disbursed Volume"]} />
              <Bar dataKey="volume" fill="#6366f1" radius={[4, 4, 0, 0]}>
                <Cell fill="#10b981" />
                <Cell fill="#6366f1" />
                <Cell fill="#f59e0b" />
              </Bar>
            </BarChart>
          </ResponsiveContainer>
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
