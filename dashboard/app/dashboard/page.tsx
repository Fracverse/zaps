"use client";

import StatCard from "@/components/StatCard";
import { api } from "@/lib/api";
import { usePolling } from "@/lib/use-polling";

export default function OverviewPage() {
  const { data, loading, error } = usePolling(() => api.socialFeed(), 15000);

  const likes = data?.reduce((total, feed) => total + feed.likes_count, 0) ?? 0;
  const comments =
    data?.reduce((total, feed) => total + feed.comments_count, 0) ?? 0;
  const activeFeeds = data?.length ?? 0;

  return (
    <div>
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-slate-900">Social Overview</h1>
        <p className="mt-1 text-sm text-slate-500">
          Live engagement across recent payment feeds.
        </p>
      </div>

      {error && (
        <div className="mb-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          {error} — showing the most recently loaded values
        </div>
      )}

      {loading && !data ? (
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

      <p className="mt-4 text-xs text-slate-400">
        Auto-refreshes every 15 seconds
      </p>
    </div>
  );
}
