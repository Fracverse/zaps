"use client";
import { usePolling } from "@/lib/use-polling";
import { api } from "@/lib/api";
import StatCard from "@/components/StatCard";

export default function OverviewPage() {
  const { data, loading, error } = usePolling(() => api.dashboardStats(), 15000);

  return (
    <div>
      <h1 className="text-2xl font-bold text-slate-900 mb-6">Overview</h1>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">
          {error} — showing cached data
        </div>
      )}

      {loading && !data ? (
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="bg-white rounded-xl border border-slate-200 p-5 h-24 animate-pulse" />
          ))}
        </div>
      ) : (
        <div className="grid grid-cols-2 lg:grid-cols-3 xl:grid-cols-5 gap-4">
          <StatCard label="Total Users" value={data?.total_users ?? 0} />
          <StatCard label="Total Payments" value={data?.total_payments ?? 0} color="text-indigo-600" />
          <StatCard label="Transfers" value={data?.total_transfers ?? 0} />
          <StatCard label="Withdrawals" value={data?.total_withdrawals ?? 0} />
          <StatCard label="Active Merchants" value={data?.active_merchants ?? 0} color="text-green-600" />
        </div>
      )}

      <p className="mt-4 text-xs text-slate-400">Auto-refreshes every 15 seconds</p>
    </div>
  );
}
