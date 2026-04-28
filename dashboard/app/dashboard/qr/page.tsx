"use client";
import { useState, useRef } from "react";
import { api } from "@/lib/api";
import QRCode from "qrcode";

export default function QRPage() {
  const [form, setForm] = useState({ merchant_id: "", amount: "", asset: "USDC", memo: "", expiry: "3600" });
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
  const [qrData, setQrData] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  const generate = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      const res = await api.generateQr({
        merchant_id: form.merchant_id,
        amount: Math.round(Number(form.amount) * 1_000_000),
        asset: form.asset,
        memo: form.memo || undefined,
        expiry: Number(form.expiry),
      });
      setQrData(res.qr_data);
      const dataUrl = await QRCode.toDataURL(res.qr_data, { width: 300, margin: 2 });
      setQrDataUrl(dataUrl);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to generate QR");
    } finally {
      setLoading(false);
    }
  };

  const download = () => {
    if (!qrDataUrl) return;
    const a = document.createElement("a");
    a.href = qrDataUrl;
    a.download = `qr-${form.merchant_id}-${Date.now()}.png`;
    a.click();
  };

  return (
    <div>
      <h1 className="text-2xl font-bold text-slate-900 mb-6">QR Code Generator</h1>
      <div className="grid md:grid-cols-2 gap-6 max-w-3xl">
        <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
          <h2 className="font-semibold text-slate-800 mb-4">Payment Details</h2>
          {error && (
            <div className="mb-3 p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">{error}</div>
          )}
          <form onSubmit={generate} className="space-y-3">
            <input
              required
              placeholder="Merchant ID"
              value={form.merchant_id}
              onChange={(e) => setForm((f) => ({ ...f, merchant_id: e.target.value }))}
              className="w-full border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
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
              placeholder="Memo (optional)"
              value={form.memo}
              onChange={(e) => setForm((f) => ({ ...f, memo: e.target.value }))}
              className="w-full border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
            <div>
              <label className="text-xs text-slate-500 mb-1 block">Expiry (seconds)</label>
              <input
                type="number"
                value={form.expiry}
                onChange={(e) => setForm((f) => ({ ...f, expiry: e.target.value }))}
                className="w-full border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
            </div>
            <button
              type="submit"
              disabled={loading}
              className="w-full bg-indigo-600 text-white py-2 rounded-lg text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
            >
              {loading ? "Generating…" : "Generate QR Code"}
            </button>
          </form>
        </div>

        <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm flex flex-col items-center justify-center">
          {qrDataUrl ? (
            <>
              <img src={qrDataUrl} alt="QR Code" className="w-64 h-64 rounded-lg" />
              <p className="mt-3 text-xs text-slate-400 font-mono break-all text-center max-w-xs">{qrData?.slice(0, 60)}…</p>
              <button
                onClick={download}
                className="mt-4 px-4 py-2 bg-slate-900 text-white text-sm rounded-lg hover:bg-slate-700 transition-colors"
              >
                ↓ Download PNG
              </button>
            </>
          ) : (
            <div className="text-center text-slate-400">
              <div className="text-6xl mb-3">⬜</div>
              <p className="text-sm">Fill in the form to generate a QR code</p>
            </div>
          )}
        </div>
      </div>
      <canvas ref={canvasRef} className="hidden" />
    </div>
  );
}
