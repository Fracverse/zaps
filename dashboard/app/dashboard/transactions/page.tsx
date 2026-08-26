"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import { format } from "date-fns";
import { api } from "@/lib/api";
import { usePolling } from "@/lib/use-polling";
import PaymentDetailDialog from "@/components/PaymentDetailDialog";
import StatusBadge from "@/components/StatusBadge";
import {
  PaginationFooter,
  PAGE_SIZE_OPTIONS,
  type PageSizeOption,
  filterTransactionRows,
} from "@/components/SearchBar";
import { downloadBlob, fmtAmount, toCSV } from "@/lib/utils";

// Default page size — overridable via the items-per-page selector (#795)
const DEFAULT_PAGE_SIZE: PageSizeOption = PAGE_SIZE_OPTIONS[0]; // 10

const visibilityStyles = {
  PUBLIC: "bg-emerald-50 text-emerald-700 ring-emerald-600/20",
  FRIENDS: "bg-amber-50 text-amber-700 ring-amber-600/20",
  PRIVATE: "bg-slate-100 text-slate-700 ring-slate-500/20",
};

function stampFilename(prefix: string, ext: string): string {
  return `${prefix}-${format(new Date(), "yyyy-MM-dd")}.${ext}`;
}

/** #787 — Export the currently displayed table rows as a local CSV or JSON file. */
function exportAuditRows(rows: unknown[], prefix: string, kind: "csv" | "json") {
  if (rows.length === 0) return;

  if (kind === "json") {
    const blob = new Blob([JSON.stringify(rows, null, 2)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = stampFilename(prefix, "json");
    anchor.click();
    URL.revokeObjectURL(url);
    return;
  }

  // Transaction rows use the shared CSV helper; social-feed rows fall back to JSON keys.
  const first = rows[0] as Record<string, unknown> | undefined;
  if (first && "from_address" in first && "send_amount" in first) {
    downloadBlob(
      toCSV(rows as Parameters<typeof toCSV>[0]),
      stampFilename(prefix, "csv"),
      "text/csv;charset=utf-8;",
    );
    return;
  }

  const headers = Object.keys(first ?? {});
  const csv = [
    headers.join(","),
    ...rows.map((row) =>
      headers
        .map((key) => `"${String((row as Record<string, unknown>)[key] ?? "").replace(/"/g, '""')}"`)
        .join(","),
    ),
  ].join("\n");
  const blob = new Blob([csv], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = stampFilename(prefix, "csv");
  anchor.click();
  URL.revokeObjectURL(url);
}

function ExportButtons({
  disabled,
  onCsv,
  onJson,
}: {
  disabled: boolean;
  onCsv: () => void;
  onJson: () => void;
}) {
  const buttonClass =
    "rounded-lg border border-slate-300 px-3 py-1.5 text-sm text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-40";
  return (
    <div className="flex items-center gap-2">
      <button type="button" disabled={disabled} onClick={onCsv} className={buttonClass}>
        Export CSV
      </button>
      <button type="button" disabled={disabled} onClick={onJson} className={buttonClass}>
        Export JSON
      </button>
    </div>
  );
}

export default function TransactionsPage() {
  return (
    <Suspense
      fallback={
        <div className="h-32 animate-pulse rounded-xl border border-slate-200 bg-white" />
      }
    >
      <TransactionsPageInner />
    </Suspense>
  );
}

function TransactionsPageInner() {
  const searchParams = useSearchParams();
  const headerQuery = searchParams.get("q") ?? "";
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  // #795 — items-per-page state
  const [pageSize, setPageSize] = useState<PageSizeOption>(DEFAULT_PAGE_SIZE);
  const [selectedUser, setSelectedUser] = useState<string | null>(null);
  const { data, loading, error, refresh } = usePolling(
    () => api.socialFeed(),
    20000,
  );
  const { data: txData } = usePolling(() => api.transactions(), 20000);

  const handleRowClick = useCallback((username: string) => {
    setSelectedUser(username);
  }, []);

  const activeQuery = search || headerQuery;

  useEffect(() => {
    setPage(0);
  }, [activeQuery]);

  const filtered = useMemo(() => {
    if (!data) return [];
    return filterTransactionRows(data, activeQuery);
  }, [data, activeQuery]);

  const filteredTx = useMemo(() => {
    if (!txData) return [];
    return filterTransactionRows(txData, activeQuery);
  }, [txData, activeQuery]);

  const totalPages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const paginated = filtered.slice(page * pageSize, (page + 1) * pageSize);

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
          placeholder="Filter by address, memo, or status"
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

      <div className="mb-6 overflow-hidden rounded-xl border border-slate-200 bg-white shadow-sm">
        <div className="flex flex-col gap-3 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
          <h2 className="text-sm font-semibold text-slate-800">Transactions</h2>
          <ExportButtons
            disabled={filteredTx.length === 0}
            onCsv={() => exportAuditRows(filteredTx, "transaction-audit", "csv")}
            onJson={() => exportAuditRows(filteredTx, "transaction-audit", "json")}
          />
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="border-b border-slate-200 bg-slate-50">
              <tr>
                {["Date", "Address", "Amount", "Memo", "Status"].map((heading) => (
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
              {!txData ? (
                Array.from({ length: 4 }).map((_, row) => (
                  <tr key={row}>
                    {Array.from({ length: 5 }).map((__, column) => (
                      <td key={column} className="px-4 py-3">
                        <div className="h-4 animate-pulse rounded bg-slate-100" />
                      </td>
                    ))}
                  </tr>
                ))
              ) : filteredTx.length === 0 ? (
                <tr>
                  <td
                    colSpan={5}
                    className="px-4 py-10 text-center text-slate-400"
                  >
                    No transactions match this filter
                  </td>
                </tr>
              ) : (
                filteredTx.map((tx) => (
                  <tr key={tx.id} className="hover:bg-slate-50">
                    <td className="whitespace-nowrap px-4 py-3 text-slate-600">
                      {format(new Date(tx.created_at), "MMM d, yyyy HH:mm")}
                    </td>
                    <td className="break-all px-4 py-3 font-mono text-xs text-slate-700">
                      {tx.from_address}
                    </td>
                    <td className="whitespace-nowrap px-4 py-3 font-medium text-slate-800">
                      {fmtAmount(tx.send_amount, tx.send_asset)}
                    </td>
                    <td className="max-w-xs px-4 py-3 text-slate-600">
                      <span className="line-clamp-2">{tx.memo || "—"}</span>
                    </td>
                    <td className="px-4 py-3">
                      <StatusBadge status={tx.status} />
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>

      <div className="overflow-hidden rounded-xl border border-slate-200 bg-white shadow-sm">
        <div className="flex flex-col gap-3 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
          <h2 className="text-sm font-semibold text-slate-800">Social payments</h2>
          <ExportButtons
            disabled={filtered.length === 0}
            onCsv={() => exportAuditRows(filtered, "social-payment-audit", "csv")}
            onJson={() => exportAuditRows(filtered, "social-payment-audit", "json")}
          />
        </div>
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
                      className="transition-colors hover:bg-slate-50 cursor-pointer"
                      onClick={() => handleRowClick(feed.sender_username)}
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
          <PaginationFooter
            total={filtered.length}
            page={page}
            pageSize={pageSize}
            onPageSizeChange={(size) => {
              setPageSize(size);
              setPage(0);
            }}
            onPageChange={setPage}
          />
        )}
      </div>
      {selectedUser && (
        <PaymentDetailDialog
          username={selectedUser}
          onClose={() => setSelectedUser(null)}
        />
      )}
    </div>
  );
}
