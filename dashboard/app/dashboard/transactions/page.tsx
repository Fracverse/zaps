"use client";

import { useMemo, useState } from "react";
import { format } from "date-fns";
import { api } from "@/lib/api";
import { usePolling } from "@/lib/use-polling";

const PAGE_SIZE = 20;

const visibilityStyles = {
  PUBLIC: "bg-emerald-50 text-emerald-700 ring-emerald-600/20",
  FRIENDS: "bg-amber-50 text-amber-700 ring-amber-600/20",
  PRIVATE: "bg-slate-100 text-slate-700 ring-slate-500/20",
};

export default function TransactionsPage() {
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const { data, loading, error, refresh } = usePolling(
    () => api.socialFeed(),
    20000,
  );

  const filtered = useMemo(() => {
    const term = search.trim().toLowerCase();
    if (!data) return [];
    if (!term) return data;

    return data.filter((feed) =>
      [
        feed.sender_username,
        feed.receiver_username,
        feed.memo,
        feed.tx_hash,
      ].some((value) => value.toLowerCase().includes(term)),
    );
  }, [data, search]);

  const totalPages = Math.ceil(filtered.length / PAGE_SIZE);
  const paginated = filtered.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);

  return (
    <div>
      <div className="mb-6 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-2xl font-bold text-slate-900">Social Payments</h1>
          <p className="mt-1 text-sm text-slate-500">
            Recent payment feeds and their social engagement.
          </p>
        </div>
        <button
          onClick={refresh}
          className="self-start rounded-lg border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-50"
        >
          ↻ Refresh
        </button>
      </div>

      <div className="mb-4 rounded-xl border border-slate-200 bg-white p-4">
        <label
          htmlFor="social-payment-search"
          className="mb-1.5 block text-xs font-semibold uppercase tracking-wide text-slate-500"
        >
          Search feeds
        </label>
        <input
          id="social-payment-search"
          placeholder="Sender, recipient, note, or transaction hash"
          value={search}
          onChange={(event) => {
            setSearch(event.target.value);
            setPage(0);
          }}
          className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
        />
      </div>

      {error && (
        <div className="mb-3 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          {error}
        </div>
      )}

      <div className="overflow-hidden rounded-xl border border-slate-200 bg-white shadow-sm">
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="border-b border-slate-200 bg-slate-50">
              <tr>
                {[
                  "Date",
                  "Feed",
                  "Amount",
                  "Note",
                  "Visibility",
                  "Likes",
                  "Comments",
                ].map((heading) => (
                  <th
                    key={heading}
                    className="px-4 py-3 text-left text-xs font-semibold uppercase tracking-wide text-slate-500"
                  >
                    {heading}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100">
              {loading && !data ? (
                Array.from({ length: 6 }).map((_, row) => (
                  <tr key={row}>
                    {Array.from({ length: 7 }).map((__, column) => (
                      <td key={column} className="px-4 py-3">
                        <div className="h-4 animate-pulse rounded bg-slate-100" />
                      </td>
                    ))}
                  </tr>
                ))
              ) : paginated.length === 0 ? (
                <tr>
                  <td
                    colSpan={7}
                    className="px-4 py-12 text-center text-slate-400"
                  >
                    No social payments found
                  </td>
                </tr>
              ) : (
                paginated.map((feed) => {
                  const visibility = feed.visibility;
                  return (
                    <tr
                      key={feed.id}
                      className="transition-colors hover:bg-slate-50"
                    >
                      <td className="whitespace-nowrap px-4 py-3 text-slate-600">
                        {format(new Date(feed.created_at), "MMM d, yyyy HH:mm")}
                      </td>
                      <td className="px-4 py-3">
                        <span className="font-medium text-slate-900">
                          {feed.sender_username}
                        </span>
                        <span className="mx-1.5 text-slate-400">→</span>
                        <span className="font-medium text-slate-900">
                          {feed.receiver_username}
                        </span>
                      </td>
                      <td className="whitespace-nowrap px-4 py-3 font-semibold text-slate-900">
                        {feed.amount} {feed.currency}
                      </td>
                      <td className="max-w-xs px-4 py-3 text-slate-600">
                        <span className="line-clamp-2">{feed.memo || "—"}</span>
                      </td>
                      <td className="px-4 py-3">
                        <span
                          className={`inline-flex rounded-full px-2.5 py-1 text-xs font-semibold ring-1 ring-inset ${visibilityStyles[visibility]}`}
                        >
                          {visibility}
                        </span>
                      </td>
                      <td className="px-4 py-3 font-medium text-pink-600">
                        ♥ {feed.likes_count}
                      </td>
                      <td className="px-4 py-3 text-slate-600">
                        {feed.comments_count}
                      </td>
                    </tr>
                  );
                })
              )}
            </tbody>
          </table>
        </div>

        {totalPages > 1 && (
          <div className="flex items-center justify-between border-t border-slate-200 bg-slate-50 px-4 py-3">
            <span className="text-xs text-slate-500">
              {filtered.length} feeds · page {page + 1} of {totalPages}
            </span>
            <div className="flex gap-1">
              <button
                disabled={page === 0}
                onClick={() => setPage((current) => current - 1)}
                className="rounded-lg border border-slate-300 px-3 py-1 text-sm hover:bg-white disabled:opacity-40"
              >
                ← Prev
              </button>
              <button
                disabled={page >= totalPages - 1}
                onClick={() => setPage((current) => current + 1)}
                className="rounded-lg border border-slate-300 px-3 py-1 text-sm hover:bg-white disabled:opacity-40"
              >
                Next →
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
