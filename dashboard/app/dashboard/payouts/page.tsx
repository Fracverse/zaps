"use client";
import { useState, useRef, useCallback } from "react";
import { usePolling } from "@/lib/use-polling";
import {
  api,
  BatchPayout,
  SdpDisbursement,
  SdpExecutionLog,
} from "@/lib/api";
import StatusBadge from "@/components/StatusBadge";
import { format } from "date-fns";
import { Upload, FileText, X, RefreshCw, AlertTriangle, CheckCircle2, ChevronDown, ChevronUp } from "lucide-react";

// Stellar address validation (G…, 56 chars, base32 subset)
const STELLAR_ADDRESS_REGEX = /^G[A-Z2-7]{55}$/;

// Expected CSV column headers for SDP disbursement files
const REQUIRED_CSV_HEADERS = ["phone", "id", "amount", "verification"];

// ── SDP status badge colours ──────────────────────────────────────────────────
function sdpStatusClass(status: string) {
  switch (status) {
    case "COMPLETED": return "bg-emerald-100 text-emerald-800";
    case "STARTED":   return "bg-blue-100 text-blue-800";
    case "READY":     return "bg-indigo-100 text-indigo-800";
    case "PAUSED":    return "bg-amber-100 text-amber-800";
    case "FAILED":    return "bg-red-100 text-red-800";
    default:          return "bg-slate-100 text-slate-600";
  }
}

function logLevelClass(level: string) {
  if (level === "error")   return "text-red-700 bg-red-50 border-red-200";
  if (level === "warning") return "text-amber-700 bg-amber-50 border-amber-200";
  return "text-slate-700 bg-slate-50 border-slate-200";
}

// ── CSV validation helper ─────────────────────────────────────────────────────
interface CsvRow {
  phone?: string;
  id?: string;
  amount?: string;
  verification?: string;
  stellar_address?: string;
  [key: string]: string | undefined;
}

function parseCSV(text: string): { headers: string[]; rows: CsvRow[] } {
  const lines = text.trim().split(/\r?\n/);
  if (lines.length < 1) return { headers: [], rows: [] };
  const headers = lines[0].split(",").map((h) => h.trim().toLowerCase());
  const rows = lines.slice(1).map((line) => {
    const values = line.split(",").map((v) => v.trim());
    const row: CsvRow = {};
    headers.forEach((h, i) => { row[h] = values[i] ?? ""; });
    return row;
  });
  return { headers, rows };
}

function validateCSV(file: File): Promise<{ ok: boolean; error?: string; rowCount?: number }> {
  return new Promise((resolve) => {
    const reader = new FileReader();
    reader.onload = (e) => {
      const text = e.target?.result as string;
      const { headers, rows } = parseCSV(text);

      // Check required headers (flexible — SDP also accepts stellar_address)
      const hasRequired = REQUIRED_CSV_HEADERS.every((h) => headers.includes(h)) ||
        (headers.includes("stellar_address") && headers.includes("amount"));

      if (!hasRequired) {
        resolve({
          ok: false,
          error: `CSV must contain columns: ${REQUIRED_CSV_HEADERS.join(", ")} — or at minimum stellar_address + amount`,
        });
        return;
      }

      // Validate Stellar addresses if present
      if (headers.includes("stellar_address")) {
        const badRow = rows.find(
          (r) => r.stellar_address && !STELLAR_ADDRESS_REGEX.test(r.stellar_address)
        );
        if (badRow) {
          resolve({
            ok: false,
            error: `Invalid Stellar address in CSV: "${badRow.stellar_address}"`,
          });
          return;
        }
      }

      resolve({ ok: true, rowCount: rows.length });
    };
    reader.onerror = () => resolve({ ok: false, error: "Failed to read file" });
    reader.readAsText(file);
  });
}

// ── SDP Execution Log Modal ───────────────────────────────────────────────────
function LogModal({
  disbursementId,
  onClose,
}: {
  disbursementId: string;
  onClose: () => void;
}) {
  const { data, loading, error } = usePolling(
    () => api.sdp.getDisbursementLogs(disbursementId),
    30000
  );
  const logs: SdpExecutionLog[] = data?.logs ?? [];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/60 p-4">
      <div className="bg-white rounded-2xl shadow-2xl max-w-3xl w-full max-h-[80vh] flex flex-col">
        {/* Header */}
        <div className="px-6 py-4 border-b border-slate-200 flex justify-between items-center">
          <div>
            <h3 className="font-semibold text-slate-900">Execution Logs</h3>
            <p className="text-xs text-slate-500 font-mono mt-0.5">
              Disbursement {disbursementId.slice(0, 8)}…
            </p>
          </div>
          <button
            onClick={onClose}
            className="text-slate-400 hover:text-slate-700 transition-colors"
            aria-label="Close log modal"
          >
            <X size={20} />
          </button>
        </div>

        {/* Body */}
        <div className="p-6 overflow-y-auto flex-1 space-y-2">
          {loading && logs.length === 0 && (
            <div className="text-center py-8 text-slate-400">Loading logs…</div>
          )}
          {error && (
            <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">
              {error}
            </div>
          )}
          {!loading && logs.length === 0 && !error && (
            <p className="text-center py-8 text-slate-400 text-sm">No execution logs yet</p>
          )}
          {logs.map((log) => (
            <div
              key={log.id}
              className={`rounded-lg border px-4 py-3 text-sm ${logLevelClass(log.level)}`}
            >
              <div className="flex items-center justify-between gap-2 mb-1">
                <span className="font-semibold uppercase text-xs tracking-wide opacity-70">
                  {log.level}
                </span>
                <span className="text-xs opacity-60">
                  {format(new Date(log.created_at), "MMM d, HH:mm:ss")}
                </span>
              </div>
              <p>{log.message}</p>
              {log.metadata && Object.keys(log.metadata).length > 0 && (
                <pre className="mt-2 text-xs opacity-60 whitespace-pre-wrap break-all">
                  {JSON.stringify(log.metadata, null, 2)}
                </pre>
              )}
            </div>
          ))}
        </div>

        <div className="px-6 py-4 border-t border-slate-200 flex justify-end">
          <button
            onClick={onClose}
            className="px-4 py-2 bg-slate-800 text-white rounded-lg text-sm font-medium hover:bg-slate-700 transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

// ── SDP Disbursement Tab ──────────────────────────────────────────────────────
function SdpDisbursementTab() {
  const [dragOver, setDragOver] = useState(false);
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [disbursementName, setDisbursementName] = useState("");
  const [validation, setValidation] = useState<{
    ok: boolean;
    error?: string;
    rowCount?: number;
  } | null>(null);
  const [uploading, setUploading] = useState(false);
  const [uploadResult, setUploadResult] = useState<{
    kind: "success";
    disbursement: SdpDisbursement;
  } | { kind: "error"; message: string } | null>(null);
  const [logsForId, setLogsForId] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const { data: sdpData, loading: sdpLoading, error: sdpError, refresh: refreshSdp } =
    usePolling(() => api.sdp.listDisbursements(20, 0), 30000);
  const disbursements: SdpDisbursement[] = sdpData?.disbursements ?? [];

  const processFile = useCallback(async (file: File) => {
    if (!file.name.endsWith(".csv")) {
      setValidation({ ok: false, error: "Only .csv files are accepted" });
      setSelectedFile(null);
      return;
    }
    setSelectedFile(file);
    setValidation(null);
    const result = await validateCSV(file);
    setValidation(result);
  }, []);

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    const file = e.dataTransfer.files?.[0];
    if (file) processFile(file);
  };

  const handleFileInput = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) processFile(file);
  };

  const handleUpload = async () => {
    if (!selectedFile || !validation?.ok) return;
    const name = disbursementName.trim() || `Batch-${Date.now()}`;
    setUploading(true);
    setUploadResult(null);
    try {
      const disbursement = await api.sdp.uploadDisbursementCSV(selectedFile, name);
      setUploadResult({ kind: "success", disbursement });
      setSelectedFile(null);
      setValidation(null);
      setDisbursementName("");
      refreshSdp();
    } catch (err) {
      setUploadResult({
        kind: "error",
        message: err instanceof Error ? err.message : "Upload failed",
      });
    } finally {
      setUploading(false);
    }
  };

  const clearFile = () => {
    setSelectedFile(null);
    setValidation(null);
    setUploadResult(null);
    if (fileInputRef.current) fileInputRef.current.value = "";
  };

  return (
    <div className="space-y-6">
      {/* CSV Upload Card */}
      <div className="bg-white border border-slate-200 rounded-2xl shadow-sm p-6">
        <div className="flex items-center gap-2 mb-1">
          <Upload size={18} className="text-indigo-600" />
          <h2 className="font-semibold text-slate-800">Upload SDP Disbursement CSV</h2>
        </div>
        <p className="text-xs text-slate-500 mb-5">
          Transmit a batch CSV to the Stellar Disbursement Platform backend. The
          request is authorised using your admin Bearer token and the
          <code className="mx-1 px-1 bg-slate-100 rounded text-slate-700">SDP-Admin-Token</code>
          credential.
        </p>

        {/* Disbursement name */}
        <div className="mb-4">
          <label htmlFor="sdp-disbursement-name" className="block text-xs font-medium text-slate-600 mb-1">
            Disbursement name (optional)
          </label>
          <input
            id="sdp-disbursement-name"
            type="text"
            placeholder={`Batch-${new Date().toISOString().slice(0, 10)}`}
            value={disbursementName}
            onChange={(e) => setDisbursementName(e.target.value)}
            className="w-full max-w-xs border border-slate-300 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
          />
        </div>

        {/* Drop Zone */}
        <div
          onDrop={handleDrop}
          onDragOver={(e) => { e.preventDefault(); setDragOver(true); }}
          onDragLeave={() => setDragOver(false)}
          onClick={() => fileInputRef.current?.click()}
          className={`cursor-pointer border-2 border-dashed rounded-xl p-8 text-center transition-all ${
            dragOver
              ? "border-indigo-500 bg-indigo-50"
              : selectedFile
              ? "border-emerald-400 bg-emerald-50"
              : "border-slate-300 hover:border-indigo-400 hover:bg-slate-50"
          }`}
        >
          <input
            ref={fileInputRef}
            type="file"
            accept=".csv"
            className="hidden"
            onChange={handleFileInput}
            id="sdp-csv-file-input"
          />
          {selectedFile ? (
            <div className="flex flex-col items-center gap-2">
              <FileText size={32} className="text-emerald-600" />
              <p className="text-sm font-medium text-slate-700">{selectedFile.name}</p>
              <p className="text-xs text-slate-500">
                {(selectedFile.size / 1024).toFixed(1)} KB
              </p>
            </div>
          ) : (
            <div className="flex flex-col items-center gap-2">
              <Upload size={32} className="text-slate-400" />
              <p className="text-sm text-slate-600">
                <span className="font-medium text-indigo-600">Browse</span> or drag &amp; drop a CSV file
              </p>
              <p className="text-xs text-slate-400">
                Expected columns: phone, id, amount, verification — or stellar_address + amount
              </p>
            </div>
          )}
        </div>

        {/* Validation feedback */}
        {validation && (
          <div
            className={`mt-3 flex items-start gap-2 rounded-lg border px-4 py-3 text-sm ${
              validation.ok
                ? "border-emerald-200 bg-emerald-50 text-emerald-700"
                : "border-red-200 bg-red-50 text-red-700"
            }`}
          >
            {validation.ok ? (
              <CheckCircle2 size={16} className="mt-0.5 shrink-0" />
            ) : (
              <AlertTriangle size={16} className="mt-0.5 shrink-0" />
            )}
            <span>
              {validation.ok
                ? `CSV validated — ${validation.rowCount} recipient row(s) ready for upload`
                : validation.error}
            </span>
          </div>
        )}

        {/* Upload result */}
        {uploadResult && (
          <div
            className={`mt-3 flex items-start gap-2 rounded-lg border px-4 py-3 text-sm ${
              uploadResult.kind === "success"
                ? "border-emerald-200 bg-emerald-50 text-emerald-700"
                : "border-red-200 bg-red-50 text-red-700"
            }`}
          >
            {uploadResult.kind === "success" ? (
              <>
                <CheckCircle2 size={16} className="mt-0.5 shrink-0" />
                <span>
                  Disbursement <strong>{uploadResult.disbursement.name}</strong> created
                  (ID: {uploadResult.disbursement.id.slice(0, 8)}…) with status{" "}
                  <strong>{uploadResult.disbursement.status}</strong>
                </span>
              </>
            ) : (
              <>
                <AlertTriangle size={16} className="mt-0.5 shrink-0" />
                <span>{uploadResult.message}</span>
              </>
            )}
          </div>
        )}

        {/* Actions */}
        <div className="mt-4 flex gap-3">
          <button
            id="sdp-upload-submit"
            onClick={handleUpload}
            disabled={!selectedFile || !validation?.ok || uploading}
            className="flex items-center gap-2 px-5 py-2 bg-indigo-600 text-white rounded-lg text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {uploading ? (
              <>
                <RefreshCw size={14} className="animate-spin" />
                Uploading…
              </>
            ) : (
              <>
                <Upload size={14} />
                Submit to SDP
              </>
            )}
          </button>
          {selectedFile && (
            <button
              onClick={clearFile}
              className="flex items-center gap-2 px-4 py-2 border border-slate-300 text-slate-600 rounded-lg text-sm font-medium hover:bg-slate-50 transition-colors"
            >
              <X size={14} />
              Clear
            </button>
          )}
        </div>
      </div>

      {/* SDP Disbursement History */}
      {sdpError && (
        <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">
          {sdpError}
        </div>
      )}
      <div className="bg-white border border-slate-200 rounded-2xl overflow-hidden shadow-sm">
        <div className="px-6 py-4 border-b border-slate-200 flex items-center justify-between">
          <h3 className="font-semibold text-slate-800">SDP Disbursement History</h3>
          <button
            onClick={refreshSdp}
            className="flex items-center gap-1 text-xs text-slate-500 hover:text-slate-700 transition-colors"
          >
            <RefreshCw size={13} />
            Refresh
          </button>
        </div>
        <table className="w-full text-sm">
          <thead className="bg-slate-50 border-b border-slate-200">
            <tr>
              {["Name", "Created", "Asset", "Payments", "Disbursed", "Status", "Logs"].map((h) => (
                <th
                  key={h}
                  className="px-6 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide"
                >
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {sdpLoading && disbursements.length === 0 ? (
              Array.from({ length: 4 }).map((_, i) => (
                <tr key={i}>
                  {Array.from({ length: 7 }).map((_, j) => (
                    <td key={j} className="px-6 py-3">
                      <div className="h-4 bg-slate-100 rounded animate-pulse" />
                    </td>
                  ))}
                </tr>
              ))
            ) : disbursements.length === 0 ? (
              <tr>
                <td colSpan={7} className="px-6 py-10 text-center text-slate-400">
                  No SDP disbursements yet — upload a CSV above to get started
                </td>
              </tr>
            ) : (
              disbursements.map((d) => (
                <tr key={d.id} className="hover:bg-slate-50">
                  <td className="px-6 py-3 font-medium text-slate-900 max-w-[180px] truncate">
                    {d.name}
                  </td>
                  <td className="px-6 py-3 text-slate-500 whitespace-nowrap">
                    {format(new Date(d.created_at), "MMM d, yyyy")}
                  </td>
                  <td className="px-6 py-3 text-slate-700">{d.asset_code}</td>
                  <td className="px-6 py-3 text-slate-600">
                    <span className="text-emerald-700">{d.successful_payments}✓</span>
                    {d.failed_payments > 0 && (
                      <span className="ml-2 text-red-600">{d.failed_payments}✗</span>
                    )}
                    <span className="ml-2 text-slate-400">/ {d.total_payments}</span>
                  </td>
                  <td className="px-6 py-3 font-medium text-slate-800">
                    {Number(d.disbursed_amount).toLocaleString()} {d.asset_code}
                  </td>
                  <td className="px-6 py-3">
                    <span
                      className={`inline-flex px-2 py-0.5 rounded-full text-xs font-medium ${sdpStatusClass(d.status)}`}
                    >
                      {d.status}
                    </span>
                  </td>
                  <td className="px-6 py-3">
                    <button
                      onClick={() => setLogsForId(d.id)}
                      className="flex items-center gap-1 text-xs text-indigo-600 hover:text-indigo-800 font-medium transition-colors"
                    >
                      View logs
                    </button>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {/* Log modal */}
      {logsForId && (
        <LogModal disbursementId={logsForId} onClose={() => setLogsForId(null)} />
      )}
    </div>
  );
}

// ── Main Page ─────────────────────────────────────────────────────────────────
export default function PayoutsPage() {
  const [activeTab, setActiveTab] = useState<"history" | "batch" | "sdp">("history");
  const [selectedBatch, setSelectedBatch] = useState<string | null>(null);

  // Batch payout history
  const {
    data: batchData,
    loading: batchLoading,
    error: batchError,
  } = usePolling(() => api.listBatchPayouts(20, 0), 30000);
  const batches: BatchPayout[] = batchData?.batches ?? [];

  // Individual payout history (legacy)
  const {
    data: payoutData,
    loading: payoutLoading,
    error: payoutError,
    refresh: refreshPayouts,
  } = usePolling(() => api.payoutHistory(20, 0), 30000);
  const payouts = payoutData?.payouts ?? [];

  // Batch details — only fetched when a batch row is clicked
  const { data: batchDetails } = selectedBatch
    ? usePolling(() => api.getBatchPayout(selectedBatch), 30000)
    : { data: null };

  const handleSubmitPayout = async (e: React.FormEvent) => {
    e.preventDefault();
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

  const TABS: { id: "history" | "batch" | "sdp"; label: string }[] = [
    { id: "history", label: "Payout History" },
    { id: "batch", label: "Batch Disbursements" },
    { id: "sdp", label: "SDP CSV Upload" },
  ];

  return (
    <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
      <div className="flex flex-col md:flex-row md:items-center md:justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-slate-900">Payouts</h1>
          <p className="mt-1 text-sm text-slate-500">
            Manage bulk disbursement batches, payout history, and SDP CSV uploads
          </p>
        </div>
      </div>

      {/* Tab navigation */}
      <div className="border-b border-slate-200 mb-6">
        <nav className="-mb-px flex space-x-8">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              onClick={() => { setActiveTab(tab.id); setSelectedBatch(null); }}
              className={`${
                activeTab === tab.id
                  ? "border-indigo-500 text-indigo-600"
                  : "border-transparent text-slate-500 hover:text-slate-700"
              } whitespace-nowrap py-4 px-1 border-b-2 font-medium text-sm transition-colors`}
            >
              {tab.label}
            </button>
          ))}
        </nav>
      </div>

      {/* ── Payout History tab ──────────────────────────────────────────────── */}
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
                    <th
                      key={h}
                      className="px-6 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide"
                    >
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
                      <td className="px-6 py-3 font-mono text-xs text-slate-500">
                        {p.id.slice(0, 8)}…
                      </td>
                      <td className="px-6 py-3 text-slate-600 whitespace-nowrap">
                        {format(new Date(p.createdAt), "MMM d, yyyy")}
                      </td>
                      <td className="px-6 py-3 font-medium">
                        {(Number(p.amount) / 1_000_000).toFixed(2)}
                      </td>
                      <td className="px-6 py-3">{p.asset}</td>
                      <td className="px-6 py-3">
                        <StatusBadge status={p.status} />
                      </td>
                      <td className="px-6 py-3 text-slate-400 text-xs">{p.anchorId}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* ── Batch Disbursements tab ─────────────────────────────────────────── */}
      {activeTab === "batch" && (
        <div className="space-y-6">
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
                {(
                  batches.reduce((sum, b) => sum + (b.total_amount || 0), 0) /
                  1_000_000
                ).toLocaleString()}
              </p>
            </div>
          </div>

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
                  {["Batch ID", "Date", "Recipients", "Total", "Status", "Results"].map(
                    (h) => (
                      <th
                        key={h}
                        className="px-6 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide"
                      >
                        {h}
                      </th>
                    )
                  )}
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
                      <td className="px-6 py-3 font-mono text-xs text-slate-500">
                        {batch.id.slice(0, 8)}…
                      </td>
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
                            {(batchDetails.batch.total_amount / 1_000_000).toLocaleString()}{" "}
                            {batchDetails.batch.currency}
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

      {/* ── SDP CSV Upload tab ──────────────────────────────────────────────── */}
      {activeTab === "sdp" && <SdpDisbursementTab />}
    </div>
  );
}
