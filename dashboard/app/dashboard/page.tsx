"use client";

import StatCard from "@/components/StatCard";
import { api } from "@/lib/api";
import { usePolling } from "@/lib/use-polling";

function fmtUsdc(value: number): string {
  return (
    value.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }) + " USDC"
  );
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

  const likes = feedData?.reduce((total, feed) => total + feed.likes_count, 0) ?? 0;
  const comments = feedData?.reduce((total, feed) => total + feed.comments_count, 0) ?? 0;
  const activeFeeds = feedData?.length ?? 0;

  const tvl = yieldData?.total_value_locked ?? 0;
  const yieldDistributed = yieldData?.total_yield_distributed ?? 0;
  const apy = yieldData?.apy ?? 0;

  return (
    <div>
      {/* Social Overview */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-slate-900">Social Overview</h1>
        <p className="mt-1 text-sm text-slate-500">
          Live engagement across recent payment feeds.
        </p>
      </div>

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
    </div>
  );
}
