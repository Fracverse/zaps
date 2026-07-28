"use client";
import { useState } from "react";
import { usePolling } from "@/lib/use-polling";
import { api, BatchPayout, BatchRecipient } from "@/lib/api";
import StatusBadge from "@/components/StatusBadge";
import { format } from "date-fns";

export default function PayoutsPage() {
  const [activeTab, setActiveTab] = useState<"history" | "batch">("history");
  const [selectedBatch, setSelectedBatch] = useState<string | null>(null);

  // Batch payout history
  const { data: batchData, loading: batchLoading, error: batchError, refresh: refreshBatches } = usePolling(
    () => api.listBatchPayouts(20, 0),
    30000
  );
  const batches: BatchPayout[] = batchData?.batches ?? [];

  // Individual payout history (legacy)
  const { data: payoutData, loading: payoutLoading, error: payoutError, refresh: refreshPayouts } = usePolling(
    () => api.payoutHistory(20, 0),
    30000
  );
  const payouts = payoutData?.payouts ?? [];

  // Batch details modal data - only fetch when batch is selected
  const { data: batchDetails, refresh: refreshBatchDetails } = selectedBatch
    ? usePolling(() => api.getBatchPayout(selectedBatch), 30000)
    : { data: null, refresh: () => {} };

  const selectedBatchRecipients = batchDetails?.recipients ?? [];

  const handleSubmitPayout = async (e: React.FormEvent) => {
    e.preventDefault();
    // Form submission logic for individual payouts
    const form = e.target as HTMLFormElement;
    const formData = new FormData(form);
    const amount = formData.get("amount") as string;
    const asset = formData.get("asset") as string;
    const bankAccountId = formData.get("bankAccountId") as string;
    const anchorId = formData.get("anchorId") as string;

    try {
      await api.requestPayout({
        amount: String(Math.round(Number(amount) * 1_000_000)),
        asset,
        bankAccountId,
        anchorId,
      });
      refreshPayouts();
    } catch (err) {
      console.error("Failed to request payout:", err);
    }
  };

  return (
    <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
      <div className="flex flex-col md:flex-row md:items-center md:justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-slate-900">Payouts</h1>
          <p className="mt-1 text-sm text-slate-500">Manage bulk disbursement batches and payout history</p>
        </div>
      </div>

      {/* Tab navigation */}
      <div className="border-b border-slate-200 mb-6">
        <nav className="-mb-px flex space-x-8">
          <button
            onClick={() => {
              setActiveTab("history");
              setSelectedBatch(null);
            }}
            className={`${
              activeTab === "history"
                ? "border-indigo-500 text-indigo-600"
                : "border-transparent text-slate-500 hover:text-slate-700"
            } whitespace-nowrap py-4 px-1 border-b-2 font-medium text-sm transition-colors`}
          >
            Payout History
          </button>
          <button
            onClick={() => setActiveTab("batch")}
            className={`${
              activeTab === "batch"
                ? "border-indigo-500 text-indigo-600"
                : "border-transparent text-slate-500 hover:text-slate-700"
            } whitespace-nowrap py-4 px-1 border-b-2 font-medium text-sm transition-colors`}
          >
            Batch Disbursements
          </button>
        </nav>
      </div>

      {activeTab === "history" && (
        <div className="space-y-6">
          {/* Request payout form */}
          <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm max-w-lg">
            <h2 className="font-semibold text-slate-800 mb-4">Request Payout</h2>
            <form onSubmit={handleSubmitPayout} className="space-y-3">
              <div className="flex gap-2">
                <input
                  required
                  type="number"
                  min="0.01"
                  step="0.01"
                  name="amount"
                  placeholder="Amount"
                  className="flex-1 border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                />
                <select
                  name="asset"
                  className="border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                >
                  {["USDC", "USDT", "XLM"].map((a) => (
                    <option key={a}>{a}</option>
                  ))}
                </select>
              </div>
              <input
                required
                name="bankAccountId"
                placeholder="Bank Account ID (UUID)"
                className="w-full border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
              <input
                required
                name="anchorId"
                placeholder="Anchor ID"
                className="w-full border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
              <button
                type="submit"
                className="w-full bg-indigo-600 text-white py-2 rounded-lg text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
              >
                Request Payout
              </button>
            </form>
          </div>

          {/* History */}
          {payoutError && (
            <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">
              {payoutError}
            </div>
          )}
          <div className="bg-white border border-slate-200 rounded-xl overflow-hidden shadow-sm">
            <div className="px-6 py-4 border-b border-slate-200">
              <h3 className="font-semibold text-slate-800">Recent Payouts</h3>
            </div>
            <table className="w-full text-sm">
              <thead className="bg-slate-50 border-b border-slate-200">
                <tr>
                  {["ID", "Date", "Amount", "Asset", "Status", "Anchor"].map((h) => (
                    <th key={h} className="px-6 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">
                      {h}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {payoutLoading && payouts.length === 0 ? (
                  Array.from({ length: 5 }).map((_, i) => (
                    <tr key={i}>
                      {Array.from({ length: 6 }).map((_, j) => (
                        <td key={j} className="px-6 py-3">
                          <div className="h-4 bg-slate-100 rounded animate-pulse" />
                        </td>
                      ))}
                    </tr>
                  ))
                ) : payouts.length === 0 ? (
                  <tr>
                    <td colSpan={6} className="px-6 py-10 text-center text-slate-400">
                      No payouts yet
                    </td>
                  </tr>
                ) : (
                  payouts.map((p) => (
                    <tr key={p.id} className="hover:bg-slate-50">
                      <td className="px-6 py-3 font-mono text-xs text-slate-500">{p.id.slice(0, 8)}…</td>
                      <td className="px-6 py-3 text-slate-600 whitespace-nowrap">
                        {format(new Date(p.createdAt), "MMM d, yyyy")}
                      </td>
                      <td className="px-6 py-3 font-medium">{(Number(p.amount) / 1_000_000).toFixed(2)}</td>
                      <td className="px-6 py-3">{p.asset}</td>
                      <td className="px-6 py-3"><StatusBadge status={p.status} /></td>
                      <td className="px-6 py-3 text-slate-400 text-xs">{p.anchorId}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {activeTab === "batch" && (
        <div className="space-y-6">
          {/* Batch payout summary cards */}
          <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
            <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
              <p className="text-sm text-slate-500">Total Batches</p>
              <p className="mt-1 text-3xl font-bold text-slate-900">{batches.length}</p>
            </div>
            <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
              <p className="text-sm text-slate-500">Total Recipients</p>
              <p className="mt-1 text-3xl font-bold text-slate-900">
                {batches.reduce((sum, b) => sum + (b.total_recipients || 0), 0)}
              </p>
            </div>
            <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
              <p className="text-sm text-slate-500">Total Volume (USDC)</p>
              <p className="mt-1 text-3xl font-bold text-slate-900">
                {(batches.reduce((sum, b) => sum + (b.total_amount || 0), 0) / 1_000_000).toLocaleString()}
              </p>
            </div>
          </div>

          {/* Batch history table */}
          {batchError && (
            <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">
              {batchError}
            </div>
          )}
          <div className="bg-white border border-slate-200 rounded-xl overflow-hidden shadow-sm">
            <div className="px-6 py-4 border-b border-slate-200">
              <h3 className="font-semibold text-slate-800">Batch Disbursement History</h3>
            </div>
            <table className="w-full text-sm">
              <thead className="bg-slate-50 border-b border-slate-200">
                <tr>
                  {["Batch ID", "Date", "Recipients", "Total", "Status", "Results"].map((h) => (
                    <th key={h} className="px-6 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">
                      {h}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {batchLoading && batches.length === 0 ? (
                  Array.from({ length: 5 }).map((_, i) => (
                    <tr key={i}>
                      {Array.from({ length: 6 }).map((_, j) => (
                        <td key={j} className="px-6 py-3">
                          <div className="h-4 bg-slate-100 rounded animate-pulse" />
                        </td>
                      ))}
                    </tr>
                  ))
                ) : batches.length === 0 ? (
                  <tr>
                    <td colSpan={6} className="px-6 py-10 text-center text-slate-400">
                      No batch disbursements yet
                    </td>
                  </tr>
                ) : (
                  batches.map((batch) => (
                    <tr
                      key={batch.id}
                      className="hover:bg-slate-50 cursor-pointer"
                      onClick={() => setSelectedBatch(batch.id)}
                    >
                      <td className="px-6 py-3 font-mono text-xs text-slate-500">{batch.id.slice(0, 8)}…</td>
                      <td className="px-6 py-3 text-slate-600 whitespace-nowrap">
                        {format(new Date(batch.created_at), "MMM d, yyyy HH:mm")}
                      </td>
                      <td className="px-6 py-3 font-medium">{batch.total_recipients}</td>
                      <td className="px-6 py-3 font-medium">
                        {(batch.total_amount / 1_000_000).toLocaleString()} {batch.currency}
                      </td>
                      <td className="px-6 py-3">
                        <StatusBadge status={batch.status} />
                      </td>
                      <td className="px-6 py-3 text-xs text-slate-500">
                        {batch.succeeded_count} succeeded, {batch.failed_count} failed
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>

          {/* Batch details modal */}
          {selectedBatch && (
            <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900 bg-opacity-50 p-4">
              <div className="bg-white rounded-xl shadow-2xl max-w-4xl w-full max-h-[80vh] flex flex-col">
                <div className="px-6 py-4 border-b border-slate-200 flex justify-between items-center">
                  <h3 className="font-semibold text-slate-800">Batch Details</h3>
                  <button
                    onClick={() => setSelectedBatch(null)}
                    className="text-slate-400 hover:text-slate-600"
                  >
                    <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>
                <div className="p-6 overflow-y-auto flex-1">
                  {batchDetails ? (
                    <div className="space-y-6">
                      <div className="grid grid-cols-2 gap-4">
                        <div>
                          <p className="text-sm text-slate-500">Batch ID</p>
                          <p className="font-mono text-sm text-slate-800">{batchDetails.batch.id}</p>
                        </div>
                        <div>
                          <p className="text-sm text-slate-500">Status</p>
                          <StatusBadge status={batchDetails.batch.status} />
                        </div>
                        <div>
                          <p className="text-sm text-slate-500">Currency</p>
                          <p className="font-medium text-slate-800">{batchDetails.batch.currency}</p>
                        </div>
                        <div>
                          <p className="text-sm text-slate-500">Total Recipients</p>
                          <p className="font-medium text-slate-800">{batchDetails.batch.total_recipients}</p>
                        </div>
                        <div>
                          <p className="text-sm text-slate-500">Total Amount</p>
                          <p className="font-medium text-slate-800">
                            {(batchDetails.batch.total_amount / 1_000_000).toLocaleString()} {batchDetails.batch.currency}
                          </p>
                        </div>
                        <div>
                          <p className="text-sm text-slate-500">Created</p>
                          <p className="font-medium text-slate-800">
                            {format(new Date(batchDetails.batch.created_at), "MMM d, yyyy HH:mm")}
                          </p>
                        </div>
                      </div>

                      {batchDetails.recipients.length > 0 && (
                        <div className="border-t border-slate-200 pt-6">
                          <h4 className="font-semibold text-slate-800 mb-3">Recipients</h4>
                          <div className="overflow-x-auto">
                            <table className="w-full text-sm">
                              <thead className="bg-slate-50">
                                <tr>
                                  <th className="px-4 py-2 text-left text-xs font-medium text-slate-500 uppercase">Status</th>
                                  <th className="px-4 py-2 text-left text-xs font-medium text-slate-500 uppercase">Amount</th>
                                  <th className="px-4 py-2 text-left text-xs font-medium text-slate-500 uppercase">Destination</th>
                                  <th className="px-4 py-2 text-left text-xs font-medium text-slate-500 uppercase">Attempts</th>
                                  <th className="px-4 py-2 text-left text-xs font-medium text-slate-500 uppercase">Tx Hash</th>
                                </tr>
                              </thead>
                              <tbody className="divide-y divide-slate-100">
                                {batchDetails.recipients.map((r) => (
                                  <tr key={r.id}>
                                    <td className="px-4 py-2"><StatusBadge status={r.status} /></td>
                                    <td className="px-4 py-2 font-medium">
                                      {(r.amount / 1_000_000).toLocaleString()}
                                    </td>
                                    <td className="px-4 py-2 text-slate-600 font-mono text-xs">
                                      {r.destination_address || r.user_id || "N/A"}
                                    </td>
                                    <td className="px-4 py-2 text-slate-600">{r.attempt_count}</td>
                                    <td className="px-4 py-2 font-mono text-xs text-slate-500">
                                      {r.tx_hash?.slice(0, 16) || "Pending"}…
                                    </td>
                                  </tr>
                                ))}
                              </tbody>
                            </table>
                          </div>
                        </div>
                      )}
                    </div>
                  ) : (
                    <div className="text-center py-8">
                      <p className="text-slate-500">Loading batch details...</p>
                    </div>
                  )}
                </div>
                <div className="px-6 py-4 border-t border-slate-200 flex justify-end">
                  <button
                    onClick={() => setSelectedBatch(null)}
                    className="px-4 py-2 bg-indigo-600 text-white rounded-lg text-sm font-medium hover:bg-indigo-700 transition-colors"
                  >
                    Close
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
