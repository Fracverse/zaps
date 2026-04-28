"use client";
import { useState } from "react";
import { usePolling } from "@/lib/use-polling";
import { api, Payout } from "@/lib/api";
import StatusBadge from "@/components/StatusBadge";
import { format } from "date-fns";

export default function PayoutsPage() {
  const { data, loading, error, refresh } = usePolling(() => api.payoutHistory(20, 0), 30000);
  const payouts: Payout[] = data?.payouts ?? [];

  const [form, setForm] = useState({ amount: "", asset: "USDC", bankAccountId: "", anchorId: "" });
  const [submitting, setSubmitting] = useState(false);
  const [msg, setMsg] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setSubmitting(true);
    setMsg(null);
    try {
      await api.requestPayout({
        amount: String(Math.round(Number(form.amount) * 1_000_000)),
        asset: form.asset,
        bankAccountId: form.bankAccountId,
        anchorId: form.anchorId,
      });
      setMsg({ type: "ok", text: "Payout requested successfully." });
      setForm({ amount: "", asset: "USDC", bankAccountId: "", anchorId: "" });
      refresh();
    } catch (e) {
      setMsg({ type: "err", text: e instanceof Error ? e.message : "Failed" });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div>
      <h1 className="text-2xl font-bold text-slate-900 mb-6">Payouts</h1>

      {/* Request payout form */}
      <div className="bg-white border border-slate-200 rounded-xl p-5 mb-6 shadow-sm max-w-lg">
        <h2 className="font-semibold text-slate-800 mb-4">Request Payout</h2>
        {msg && (
          <div className={`mb-3 p-3 rounded-lg text-sm ${msg.type === "ok" ? "bg-green-50 text-green-700 border border-green-200" : "bg-red-50 text-red-700 border border-red-200"}`}>
            {msg.text}
          </div>
        )}
        <form onSubmit={submit} className="space-y-3">
          <div className="flex gap-2">
            <input
              required
              type="number"
              min="0.01"
              step="0.01"
              placeholder="Amount"
              value={form.amount}
              onChange={(e) => setForm((f) => ({ ...f, amount: e.target.value }))}
              className="flex-1 border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
            <select
              value={form.asset}
              onChange={(e) => setForm((f) => ({ ...f, asset: e.target.value }))}
              className="border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            >
              {["USDC", "USDT", "XLM"].map((a) => <option key={a}>{a}</option>)}
            </select>
          </div>
          <input
            required
            placeholder="Bank Account ID (UUID)"
            value={form.bankAccountId}
            onChange={(e) => setForm((f) => ({ ...f, bankAccountId: e.target.value }))}
            className="w-full border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
          />
          <input
            required
            placeholder="Anchor ID"
            value={form.anchorId}
            onChange={(e) => setForm((f) => ({ ...f, anchorId: e.target.value }))}
            className="w-full border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
          />
          <button
            type="submit"
            disabled={submitting}
            className="w-full bg-indigo-600 text-white py-2 rounded-lg text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
          >
            {submitting ? "Requesting…" : "Request Payout"}
          </button>
        </form>
      </div>

      {/* History */}
      <h2 className="font-semibold text-slate-800 mb-3">Payout History</h2>
      {error && <div className="mb-3 p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">{error}</div>}
      <div className="bg-white border border-slate-200 rounded-xl overflow-hidden shadow-sm">
        <table className="w-full text-sm">
          <thead className="bg-slate-50 border-b border-slate-200">
            <tr>
              {["ID", "Date", "Amount", "Asset", "Status", "Anchor"].map((h) => (
                <th key={h} className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">{h}</th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {loading && payouts.length === 0 ? (
              Array.from({ length: 5 }).map((_, i) => (
                <tr key={i}>{Array.from({ length: 6 }).map((_, j) => (
                  <td key={j} className="px-4 py-3"><div className="h-4 bg-slate-100 rounded animate-pulse" /></td>
                ))}</tr>
              ))
            ) : payouts.length === 0 ? (
              <tr><td colSpan={6} className="px-4 py-10 text-center text-slate-400">No payouts yet</td></tr>
            ) : (
              payouts.map((p) => (
                <tr key={p.id} className="hover:bg-slate-50">
                  <td className="px-4 py-3 font-mono text-xs text-slate-500">{p.id.slice(0, 8)}…</td>
                  <td className="px-4 py-3 text-slate-600 whitespace-nowrap">{format(new Date(p.createdAt), "MMM d, yyyy")}</td>
                  <td className="px-4 py-3 font-medium">{(Number(p.amount) / 1_000_000).toFixed(2)}</td>
                  <td className="px-4 py-3">{p.asset}</td>
                  <td className="px-4 py-3"><StatusBadge status={p.status} /></td>
                  <td className="px-4 py-3 text-slate-400 text-xs">{p.anchorId}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
