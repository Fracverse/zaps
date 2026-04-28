"use client";
import { useState, useMemo } from "react";
import { usePolling } from "@/lib/use-polling";
import { api, Transaction } from "@/lib/api";
import StatusBadge from "@/components/StatusBadge";
import { fmtAmount, toCSV, downloadBlob } from "@/lib/utils";
import { format } from "date-fns";

const PAGE_SIZE = 20;
const STATUSES = ["", "pending", "processing", "completed", "failed", "refunded"];

export default function TransactionsPage() {
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("");
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [minAmt, setMinAmt] = useState("");
  const [maxAmt, setMaxAmt] = useState("");
  const [page, setPage] = useState(0);

  const params = useMemo(() => {
    const p: Record<string, string> = {};
    if (status) p.status = status;
    if (dateFrom) p.date_from = dateFrom;
    if (dateTo) p.date_to = dateTo;
    return p;
  }, [status, dateFrom, dateTo]);

  const { data: raw, loading, error, refresh } = usePolling(
    () => api.transactions(params),
    20000,
    [JSON.stringify(params)]
  );

  const filtered = useMemo(() => {
    if (!raw) return [];
    return raw.filter((t) => {
      if (search && !t.id.includes(search) && !t.from_address.includes(search) && !t.memo?.includes(search)) return false;
      if (minAmt && t.send_amount < Number(minAmt) * 1_000_000) return false;
      if (maxAmt && t.send_amount > Number(maxAmt) * 1_000_000) return false;
      return true;
    });
  }, [raw, search, minAmt, maxAmt]);

  const paginated = filtered.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);
  const totalPages = Math.ceil(filtered.length / PAGE_SIZE);

  const exportCSV = () => {
    downloadBlob(toCSV(filtered), `transactions-${Date.now()}.csv`, "text/csv");
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold text-slate-900">Transactions</h1>
        <div className="flex gap-2">
          <button onClick={refresh} className="px-3 py-1.5 text-sm border border-slate-300 rounded-lg hover:bg-slate-50">
            ↻ Refresh
          </button>
          <button onClick={exportCSV} className="px-3 py-1.5 text-sm bg-indigo-600 text-white rounded-lg hover:bg-indigo-700">
            ↓ Export CSV
          </button>
        </div>
      </div>

      {/* Filters */}
      <div className="bg-white border border-slate-200 rounded-xl p-4 mb-4 grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3">
        <input
          placeholder="Search ID / address"
          value={search}
          onChange={(e) => { setSearch(e.target.value); setPage(0); }}
          className="col-span-2 border border-slate-300 rounded-lg px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
        />
        <select
          value={status}
          onChange={(e) => { setStatus(e.target.value); setPage(0); }}
          className="border border-slate-300 rounded-lg px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
        >
          {STATUSES.map((s) => (
            <option key={s} value={s}>{s || "All statuses"}</option>
          ))}
        </select>
        <input
          type="date"
          value={dateFrom}
          onChange={(e) => { setDateFrom(e.target.value); setPage(0); }}
          className="border border-slate-300 rounded-lg px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
        />
        <input
          type="date"
          value={dateTo}
          onChange={(e) => { setDateTo(e.target.value); setPage(0); }}
          className="border border-slate-300 rounded-lg px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
        />
        <div className="flex gap-1">
          <input
            placeholder="Min $"
            value={minAmt}
            onChange={(e) => { setMinAmt(e.target.value); setPage(0); }}
            className="w-full border border-slate-300 rounded-lg px-2 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
          />
          <input
            placeholder="Max $"
            value={maxAmt}
            onChange={(e) => { setMaxAmt(e.target.value); setPage(0); }}
            className="w-full border border-slate-300 rounded-lg px-2 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
          />
        </div>
      </div>

      {error && (
        <div className="mb-3 p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">{error}</div>
      )}

      {/* Table */}
      <div className="bg-white border border-slate-200 rounded-xl overflow-hidden shadow-sm">
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="bg-slate-50 border-b border-slate-200">
              <tr>
                {["ID", "Date", "From", "Asset", "Amount", "Status", "Memo"].map((h) => (
                  <th key={h} className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100">
              {loading && !raw ? (
                Array.from({ length: 8 }).map((_, i) => (
                  <tr key={i}>
                    {Array.from({ length: 7 }).map((_, j) => (
                      <td key={j} className="px-4 py-3">
                        <div className="h-4 bg-slate-100 rounded animate-pulse" />
                      </td>
                    ))}
                  </tr>
                ))
              ) : paginated.length === 0 ? (
                <tr>
                  <td colSpan={7} className="px-4 py-10 text-center text-slate-400">
                    No transactions found
                  </td>
                </tr>
              ) : (
                paginated.map((t) => (
                  <tr key={t.id} className="hover:bg-slate-50 transition-colors">
                    <td className="px-4 py-3 font-mono text-xs text-slate-500">{t.id.slice(0, 8)}…</td>
                    <td className="px-4 py-3 text-slate-600 whitespace-nowrap">
                      {format(new Date(t.created_at), "MMM d, yyyy HH:mm")}
                    </td>
                    <td className="px-4 py-3 font-mono text-xs text-slate-500">{t.from_address.slice(0, 12)}…</td>
                    <td className="px-4 py-3 font-medium">{t.send_asset}</td>
                    <td className="px-4 py-3 font-medium">{fmtAmount(t.send_amount, t.send_asset)}</td>
                    <td className="px-4 py-3"><StatusBadge status={t.status} /></td>
                    <td className="px-4 py-3 text-slate-400 text-xs">{t.memo ?? "—"}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>

        {/* Pagination */}
        {totalPages > 1 && (
          <div className="flex items-center justify-between px-4 py-3 border-t border-slate-200 bg-slate-50">
            <span className="text-xs text-slate-500">
              {filtered.length} results · page {page + 1} of {totalPages}
            </span>
            <div className="flex gap-1">
              <button
                disabled={page === 0}
                onClick={() => setPage((p) => p - 1)}
                className="px-3 py-1 text-sm border border-slate-300 rounded-lg disabled:opacity-40 hover:bg-white"
              >
                ← Prev
              </button>
              <button
                disabled={page >= totalPages - 1}
                onClick={() => setPage((p) => p + 1)}
                className="px-3 py-1 text-sm border border-slate-300 rounded-lg disabled:opacity-40 hover:bg-white"
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
