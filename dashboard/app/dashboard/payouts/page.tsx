"use client";

import { useCallback, useMemo, useRef, useState } from "react";
import { useDropzone } from "react-dropzone";
import Papa from "papaparse";
import { format } from "date-fns";
import {
  Upload,
  FileText,
  X,
  RefreshCw,
  AlertTriangle,
  CheckCircle2,
  Info,
} from "lucide-react";
import { usePolling } from "@/lib/use-polling";
import {
  api,
  type BatchPayout,
  type BatchRecipient,
  type Payout,
  type SdpDisbursement,
} from "@/lib/api";
import StatusBadge from "@/components/StatusBadge";

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

const STELLAR_ADDRESS_REGEX = /^G[A-Z2-7]{55}$/;

// SDP accepts flexible column sets. We validate against known accepted headers
// so admins don't have to guess.
const SDP_HEADER_ALIASES: { canonical: string; accepted: string[] }[] = [
  { canonical: "phone", accepted: ["phone", "phone_number", "msisdn"] },
  { canonical: "id", accepted: ["id", "user_id", "identifier"] },
  { canonical: "amount", accepted: ["amount"] },
  { canonical: "verification", accepted: ["verification", "verification_code", "code"] },
  { canonical: "stellar_address", accepted: ["stellar_address", "recipient_address", "destination", "destination_address"] },
  { canonical: "currency", accepted: ["currency", "asset", "asset_code"] },
  { canonical: "recipient_name", accepted: ["recipient_name", "name", "recipient"] },
];

const REQUIRED_CSV_HEADER_GROUPS: { label: string; required: string[] }[] = [
  { label: "phone + id + amount + verification", required: ["phone", "id", "amount", "verification"] },
  { label: "stellar_address + amount", required: ["stellar_address", "amount"] },
];

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

type PayoutTab = "history" | "batch" | "sdp";

interface ParsedCsvRow {
  __rowNumber: number;
  [key: string]: unknown;
}

interface ValidatedRow {
  rowNumber: number;
  raw: ParsedCsvRow;
  valid: boolean;
  errors: string[];
  stellarAddress?: string;
  amount?: number;
  currency?: string;
}

interface ParsedCsvSummary {
  fileName: string;
  fileSizeKb: number;
  headers: string[];
  totalRows: number;
  validRows: number;
  invalidRows: number;
  headerErrors: string[];
  amountTotal?: number;
  currencyCounts: Record<string, number>;
  addressFoundCount: number;
  rows: ValidatedRow[];
}

type ValidationStatus =
  | { kind: "idle" }
  | { kind: "parsing" }
  | { kind: "ok"; summary: ParsedCsvSummary }
  | { kind: "error"; message: string };

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

function normalizeHeader(raw: string): string {
  const clean = raw.trim().toLowerCase();
  for (const group of SDP_HEADER_ALIASES) {
    if (group.accepted.includes(clean)) return group.canonical;
  }
  return clean;
}

function buildHeaderMap(rawHeaders: string[]): Map<string, number> {
  const map = new Map<string, number>();
  for (let i = 0; i < rawHeaders.length; i++) {
    map.set(normalizeHeader(rawHeaders[i]), i);
  }
  return map;
}

function getCell(row: ParsedCsvRow, column: string): string {
  const raw = row[column];
  if (raw === null || raw === undefined) return "";
  return String(raw).trim();
}

function detectHeaderErrors(headers: string[]): string[] {
  const normalized = headers.map(normalizeHeader);
  const matched = REQUIRED_CSV_HEADER_GROUPS.find((g) =>
    g.required.every((h) => normalized.includes(h)),
  );
  if (matched) return [];
  const groupStr = REQUIRED_CSV_HEADER_GROUPS.map(
    (g) => "[" + g.required.join(", ") + "] (" + g.label + ")",
  ).join(" OR ");
  return [
    `Missing required column group. SDP expects any of: ${groupStr}.`,
    `Headers found: ${headers.join(", ") || "(none)"}`,
  ];
}

function validateRows(
  rows: ParsedCsvRow[],
  headers: string[],
): ValidatedRow[] {
  const headerMap = buildHeaderMap(headers);
  return rows.map((row) => {
    const errors: string[] = [];
    const rowNumber = row.__rowNumber;

    // Figure out which cells are provided
    const stellarAddress = headerMap.has("stellar_address")
      ? getCell(row, headers[headerMap.get("stellar_address")!])
      : "";
    const phone = headerMap.has("phone")
      ? getCell(row, headers[headerMap.get("phone")!])
      : "";
    const id = headerMap.has("id")
      ? getCell(row, headers[headerMap.get("id")!])
      : "";
    const verification = headerMap.has("verification")
      ? getCell(row, headers[headerMap.get("verification")!])
      : "";
    const amountRaw = headerMap.has("amount")
      ? getCell(row, headers[headerMap.get("amount")!])
      : "";
    const currency = headerMap.has("currency")
      ? getCell(row, headers[headerMap.get("currency")!]).toUpperCase()
      : undefined;

    // Mode 1: stellar_address route
    const hasStellar = !!stellarAddress;
    // Mode 2: phone/id route
    const hasPhoneId = !!(phone && id && verification);

    if (!hasStellar && !hasPhoneId) {
      errors.push(
        "Row must provide either stellar_address OR (phone + id + verification).",
      );
    }

    if (hasStellar && !STELLAR_ADDRESS_REGEX.test(stellarAddress)) {
      errors.push("stellar_address is not a valid G-address.");
    }

    const num = Number(amountRaw);
    if (!amountRaw) {
      errors.push("amount is required.");
    } else if (!Number.isFinite(num) || num <= 0) {
      errors.push("amount must be a positive number.");
    }

    return {
      rowNumber,
      raw: row,
      valid: errors.length === 0,
      errors,
      stellarAddress: stellarAddress || undefined,
      amount: Number.isFinite(num) && num > 0 ? num : undefined,
      currency: currency || undefined,
    };
  });
}

function parseCsvFile(file: File): Promise<Papa.ParseResult<ParsedCsvRow>> {
  return new Promise((resolve, reject) => {
    Papa.parse<ParsedCsvRow>(file, {
      header: true,
      skipEmptyLines: true,
      transformHeader: (h) => h,
      complete: (results) => resolve(results),
      error: (err) => reject(err),
    });
  });
}

// ──────────────────────────────────────────────────────────────────────────────
// Sub-components
// ──────────────────────────────────────────────────────────────────────────────

function LogModal({
  disbursementId,
  onClose,
}: {
  disbursementId: string;
  onClose: () => void;
}) {
  const { data, loading, error } = usePolling(
    () => api.sdp.getDisbursementLogs(disbursementId),
    10000,
  );
  const logs = data?.logs ?? [];

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="flex max-h-[85vh] w-full max-w-4xl flex-col overflow-hidden rounded-2xl bg-white shadow-2xl">
        <div className="flex items-center justify-between border-b border-slate-200 px-6 py-4">
          <div>
            <h3 className="text-lg font-semibold text-slate-900">
              Disbursement Logs
            </h3>
            <p className="mt-0.5 font-mono text-xs text-slate-500">
              {disbursementId}
            </p>
          </div>
          <button
            onClick={onClose}
            className="rounded-lg p-1.5 text-slate-400 transition hover:bg-slate-100 hover:text-slate-600"
            aria-label="Close"
          >
            <X size={18} />
          </button>
        </div>
        <div className="flex-1 overflow-y-auto px-6 py-4">
          {loading && logs.length === 0 ? (
            <div className="py-12 text-center text-sm text-slate-400">
              Loading logs…
            </div>
          ) : error ? (
            <div className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700">
              {error}
            </div>
          ) : logs.length === 0 ? (
            <div className="py-12 text-center text-sm text-slate-400">
              No logs recorded yet.
            </div>
          ) : (
            <ul className="space-y-2">
              {logs.map((l) => {
                const tone =
                  l.level === "error"
                    ? "border-red-200 bg-red-50"
                    : l.level === "warning"
                      ? "border-amber-200 bg-amber-50"
                      : "border-slate-200 bg-white";
                return (
                  <li
                    key={l.id}
                    className={`rounded-lg border p-3 text-sm ${tone}`}
                  >
                    <div className="mb-1 flex items-center justify-between text-xs">
                      <span
                        className={`rounded-full px-2 py-0.5 font-semibold uppercase ${
                          l.level === "error"
                            ? "bg-red-200 text-red-900"
                            : l.level === "warning"
                              ? "bg-amber-200 text-amber-900"
                              : "bg-slate-200 text-slate-800"
                        }`}
                      >
                        {l.level}
                      </span>
                      <span className="text-slate-500">
                        {format(new Date(l.created_at), "MMM d, HH:mm:ss")}
                      </span>
                    </div>
                    <p className="text-slate-800">{l.message}</p>
                    {l.metadata &&
                      Object.keys(l.metadata as object).length > 0 && (
                        <pre className="mt-2 max-h-40 overflow-auto rounded bg-slate-900 p-2 text-[11px] leading-relaxed text-slate-100">
                          {JSON.stringify(l.metadata, null, 2)}
                        </pre>
                      )}
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// SDP CSV Upload Tab — the focus of this task
// ──────────────────────────────────────────────────────────────────────────────

function SdpDisbursementTab() {
  const [
    disbursements,
    setDisbursements,
  ] = useState<SdpDisbursement[] | null>(null);
  const [disbursementName, setDisbursementName] = useState("");
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [validation, setValidation] = useState<ValidationStatus>({
    kind: "idle",
  });
  const [uploading, setUploading] = useState(false);
  const [
    uploadResult,
    setUploadResult,
  ] = useState<null | { kind: "success"; disbursement: SdpDisbursement } | {
    kind: "error";
    message: string;
  }>(null);
  const [logsForId, setLogsForId] = useState<string | null>(null);

  const onDrop = useCallback(
    async (acceptedFiles: File[]) => {
      setUploadResult(null);
      const file = acceptedFiles[0];
      if (!file) return;
      setSelectedFile(file);
      setValidation({ kind: "parsing" });

      try {
        const parseResult = await parseCsvFile(file);
        const rawHeaders = (parseResult.meta.fields ?? []) as string[];
        const headerErrors = detectHeaderErrors(rawHeaders);

        const dataRows: ParsedCsvRow[] = (parseResult.data ?? []).map(
          (row, idx) => ({
            ...row,
            __rowNumber: idx + 2, // +1 for header, +1 for 1-indexed
          }),
        );

        const parseErrors: string[] = [
          ...headerErrors,
          ...(parseResult.errors ?? []).map(
            (e) =>
              `Parse${e.row !== undefined ? ` (row ${e.row + 1})` : ""}: ${e.message}`,
          ),
        ];

        const validatedRows =
          headerErrors.length === 0 ? validateRows(dataRows, rawHeaders) : [];

        const validCount = validatedRows.filter((r) => r.valid).length;
        const invalidCount = validatedRows.length - validCount;

        const currencyCounts: Record<string, number> = {};
        let amountTotal = 0;
        let addressFoundCount = 0;
        for (const r of validatedRows) {
          if (!r.valid) continue;
          if (r.currency) {
            currencyCounts[r.currency] = (currencyCounts[r.currency] ?? 0) + 1;
          } else {
            currencyCounts["(default)"] =
              (currencyCounts["(default)"] ?? 0) + 1;
          }
          if (r.amount !== undefined) amountTotal += r.amount;
          if (r.stellarAddress) addressFoundCount += 1;
        }

        const summary: ParsedCsvSummary = {
          fileName: file.name,
          fileSizeKb: file.size / 1024,
          headers: rawHeaders,
          totalRows: dataRows.length,
          validRows: validCount,
          invalidRows: invalidCount,
          headerErrors: parseErrors,
          amountTotal: amountTotal > 0 ? amountTotal : undefined,
          currencyCounts,
          addressFoundCount,
          rows: validatedRows,
        };

        setValidation({ kind: "ok", summary });
      } catch (err) {
        setValidation({
          kind: "error",
          message:
            err instanceof Error ? err.message : "Failed to parse CSV file.",
        });
      }
    },
    [],
  );

  const { getRootProps, getInputProps, isDragActive } = useDropzone({
    onDrop,
    accept: {
      "text/csv": [".csv"],
      "application/vnd.ms-excel": [".csv"],
    },
    multiple: false,
    maxFiles: 1,
  });

  const clearFile = () => {
    setSelectedFile(null);
    setValidation({ kind: "idle" });
    setUploadResult(null);
  };

  const refreshSdp = useCallback(async () => {
    try {
      const res = await api.sdp.listDisbursements(20, 0);
      setDisbursements(res.disbursements);
    } catch {
      /* silently ignore — stale state is fine */
    }
  }, []);

  // Initial + periodic SDP listing
  usePolling(
    () =>
      api.sdp.listDisbursements(20, 0).then((res) => {
        setDisbursements(res.disbursements);
        return res;
      }),
    20000,
  );

  const summary = validation.kind === "ok" ? validation.summary : null;
  const submissionAllowed =
    !!selectedFile &&
    validation.kind === "ok" &&
    summary &&
    summary.headerErrors.length === 0 &&
    summary.invalidRows === 0;

  const handleUpload = async () => {
    if (!selectedFile || !submissionAllowed) return;
    setUploading(true);
    setUploadResult(null);
    const name =
      disbursementName.trim() ||
      `${fileBasename(selectedFile.name)}-${Date.now()}`;
    try {
      const disbursement = await api.sdp.uploadDisbursementCSV(
        selectedFile,
        name,
      );
      setUploadResult({ kind: "success", disbursement });
      clearFile();
      setDisbursementName("");
      await refreshSdp();
    } catch (err) {
      setUploadResult({
        kind: "error",
        message: err instanceof Error ? err.message : "Upload failed.",
      });
    } finally {
      setUploading(false);
    }
  };

  return (
    <div className="space-y-8">
      {/* ── Upload Card ───────────────────────────────────────────────────── */}
      <section className="rounded-2xl border border-slate-200 bg-white p-6 shadow-sm">
        <div className="mb-1 flex items-center gap-2">
          <Upload size={18} className="text-indigo-600" />
          <h2 className="font-semibold text-slate-800">
            Upload SDP Disbursement CSV
          </h2>
        </div>
        <p className="mb-5 text-xs text-slate-500">
          Drop a CSV file to validate recipients, amounts, and addresses before
          sending to the Stellar Disbursement Platform.
          <code className="mx-1 rounded bg-slate-100 px-1 text-slate-700">
            SDP-Admin-Token
          </code>
          and admin Bearer auth are applied automatically.
        </p>

        <div className="mb-4">
          <label
            htmlFor="sdp-disbursement-name"
            className="mb-1 block text-xs font-medium text-slate-600"
          >
            Disbursement name{" "}
            <span className="text-slate-400">(optional)</span>
          </label>
          <input
            id="sdp-disbursement-name"
            type="text"
            placeholder={`Batch-${new Date().toISOString().slice(0, 10)}`}
            value={disbursementName}
            onChange={(e) => setDisbursementName(e.target.value)}
            className="w-full max-w-md rounded-lg border border-slate-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
          />
        </div>

        {/* react-dropzone drop zone */}
        <div
          {...getRootProps()}
          className={`cursor-pointer rounded-xl border-2 border-dashed p-8 text-center transition-all ${
            isDragActive
              ? "border-indigo-500 bg-indigo-50"
              : selectedFile
                ? "border-emerald-400 bg-emerald-50"
                : "border-slate-300 hover:border-indigo-400 hover:bg-slate-50"
          }`}
        >
          <input {...getInputProps()} />
          {selectedFile ? (
            <div className="flex flex-col items-center gap-2">
              <FileText size={32} className="text-emerald-600" />
              <p className="text-sm font-medium text-slate-700">
                {selectedFile.name}
              </p>
              <p className="text-xs text-slate-500">
                {(selectedFile.size / 1024).toFixed(1)} KB
              </p>
              {isDragActive && (
                <p className="text-xs text-indigo-700 font-medium">
                  Drop to replace…
                </p>
              )}
            </div>
          ) : (
            <div className="flex flex-col items-center gap-2">
              <Upload size={32} className="text-slate-400" />
              <p className="text-sm text-slate-600">
                <span className="font-medium text-indigo-600">Browse</span> or
                drag &amp; drop a CSV file
              </p>
              <p className="text-xs text-slate-400">
                Required columns:
                <span className="mx-1 font-mono">
                  stellar_address, amount
                </span>
                OR
                <span className="mx-1 font-mono">
                  phone, id, amount, verification
                </span>
              </p>
            </div>
          )}
        </div>

        {/* ── Validation summary (parsing state, error, or detail) ──────── */}
        {validation.kind === "parsing" && (
          <div className="mt-3 flex items-start gap-2 rounded-lg border border-indigo-200 bg-indigo-50 px-4 py-3 text-sm text-indigo-700">
            <RefreshCw size={16} className="mt-0.5 shrink-0 animate-spin" />
            <span>Parsing CSV with PapaParse…</span>
          </div>
        )}

        {validation.kind === "error" && (
          <div className="mt-3 flex items-start gap-2 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
            <AlertTriangle size={16} className="mt-0.5 shrink-0" />
            <span>{validation.message}</span>
          </div>
        )}

        {validation.kind === "ok" && summary && (
          <div className="mt-4 space-y-4">
            {/* ── Stat cards ──────────────────────────────────────────── */}
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              <StatTile label="Rows parsed" value={summary.totalRows.toString()} tone="slate" />
              <StatTile
                label="Valid"
                value={summary.validRows.toString()}
                tone={summary.invalidRows === 0 ? "emerald" : "slate"}
              />
              <StatTile
                label="Invalid"
                value={summary.invalidRows.toString()}
                tone={summary.invalidRows > 0 ? "red" : "slate"}
              />
              <StatTile
                label="Currency groups"
                value={Object.keys(summary.currencyCounts).length.toString()}
                tone="indigo"
              />
            </div>

            {/* ── Header validation banner ───────────────────────────── */}
            {summary.headerErrors.length > 0 ? (
              <Banner
                tone="error"
                icon={<AlertTriangle size={16} />}
                title="CSV header validation failed"
                lines={summary.headerErrors}
              />
            ) : summary.invalidRows > 0 ? (
              <Banner
                tone="warning"
                icon={<AlertTriangle size={16} />}
                title={`${summary.invalidRows} row(s) have validation issues — fix before submitting`}
                lines={[
                  `Review the rows table below. ${summary.validRows} of ${summary.totalRows} rows are valid.`,
                ]}
              />
            ) : (
              <Banner
                tone="success"
                icon={<CheckCircle2 size={16} />}
                title={`${summary.validRows} recipient row(s) ready for upload`}
                lines={[
                  summary.amountTotal !== undefined
                    ? `Total amount across valid rows: ${summary.amountTotal.toLocaleString()} (currency counts: ${Object.entries(summary.currencyCounts).map(([k, v]) => `${k} × ${v}`).join(", ")})`
                    : "Amounts parsed successfully.",
                  summary.addressFoundCount > 0
                    ? `${summary.addressFoundCount} row(s) target Stellar addresses directly.`
                    : "Rows target phone/id route (phone + id + verification).",
                ]}
              />
            )}

            {/* ── Invalid preview (if any) ───────────────────────────── */}
            {summary.rows.some((r) => !r.valid) && (
              <div className="overflow-hidden rounded-xl border border-slate-200">
                <div className="flex items-center justify-between border-b border-slate-200 bg-slate-50 px-4 py-2.5">
                  <h3 className="text-xs font-semibold uppercase tracking-wide text-slate-600">
                    Validation issues ({summary.invalidRows} rows)
                  </h3>
                </div>
                <div className="max-h-72 overflow-auto">
                  <table className="w-full text-xs">
                    <thead className="bg-slate-50 text-slate-500">
                      <tr>
                        <th className="px-3 py-2 text-left font-semibold">
                          Row
                        </th>
                        <th className="px-3 py-2 text-left font-semibold">
                          Target
                        </th>
                        <th className="px-3 py-2 text-left font-semibold">
                          Amount
                        </th>
                        <th className="px-3 py-2 text-left font-semibold">
                          Errors
                        </th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-100">
                      {summary.rows
                        .filter((r) => !r.valid)
                        .slice(0, 100)
                        .map((r) => (
                          <tr key={r.rowNumber} className="bg-red-50/40">
                            <td className="px-3 py-2 font-mono text-slate-500">
                              #{r.rowNumber}
                            </td>
                            <td className="px-3 py-2 font-mono text-slate-700 break-all">
                              {r.stellarAddress ??
                                (r.raw as any).phone ??
                                (r.raw as any).id ??
                                "—"}
                            </td>
                            <td className="px-3 py-2 text-slate-700">
                              {r.amount !== undefined
                                ? r.amount.toString()
                                : (r.raw as any).amount ?? "—"}
                            </td>
                            <td className="px-3 py-2">
                              <ul className="list-disc pl-4 space-y-0.5 text-red-700">
                                {r.errors.map((e, idx) => (
                                  <li key={idx}>{e}</li>
                                ))}
                              </ul>
                            </td>
                          </tr>
                        ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}

            {/* ── Valid rows summary table ───────────────────────────── */}
            {summary.rows.some((r) => r.valid) && (
              <div className="overflow-hidden rounded-xl border border-slate-200">
                <div className="flex items-center justify-between border-b border-slate-200 bg-slate-50 px-4 py-2.5">
                  <h3 className="text-xs font-semibold uppercase tracking-wide text-slate-600">
                    Valid rows ({summary.validRows})
                  </h3>
                  <span className="text-xs text-slate-400">
                    Showing first{" "}
                    {Math.min(summary.validRows, 50).toString()}
                  </span>
                </div>
                <div className="max-h-64 overflow-auto">
                  <table className="w-full text-xs">
                    <thead className="sticky top-0 bg-slate-50 text-slate-500">
                      <tr>
                        <th className="px-3 py-2 text-left font-semibold">
                          Row
                        </th>
                        <th className="px-3 py-2 text-left font-semibold">
                          Destination
                        </th>
                        <th className="px-3 py-2 text-right font-semibold">
                          Amount
                        </th>
                        <th className="px-3 py-2 text-left font-semibold">
                          Currency
                        </th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-100">
                      {summary.rows
                        .filter((r) => r.valid)
                        .slice(0, 50)
                        .map((r) => (
                          <tr key={r.rowNumber}>
                            <td className="px-3 py-2 font-mono text-slate-500">
                              #{r.rowNumber}
                            </td>
                            <td className="px-3 py-2 font-mono text-slate-700 break-all">
                              {r.stellarAddress ??
                                (r.raw as any).phone ??
                                (r.raw as any).id ??
                                "—"}
                            </td>
                            <td className="px-3 py-2 text-right font-medium text-slate-800">
                              {r.amount?.toLocaleString() ?? "—"}
                            </td>
                            <td className="px-3 py-2 text-slate-700">
                              {r.currency ?? "—"}
                            </td>
                          </tr>
                        ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </div>
        )}

        {/* Upload result */}
        {uploadResult?.kind === "success" && (
          <div className="mt-3 flex items-start gap-2 rounded-lg border border-emerald-200 bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
            <CheckCircle2 size={16} className="mt-0.5 shrink-0" />
            <div>
              <p className="font-semibold">Disbursement created</p>
              <p>
                <strong>{uploadResult.disbursement.name}</strong> ·{" "}
                <code>{uploadResult.disbursement.id.slice(0, 12)}…</code> ·
                status:{" "}
                <strong>{uploadResult.disbursement.status}</strong>
              </p>
            </div>
          </div>
        )}
        {uploadResult?.kind === "error" && (
          <div className="mt-3 flex items-start gap-2 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
            <AlertTriangle size={16} className="mt-0.5 shrink-0" />
            <span>{uploadResult.message}</span>
          </div>
        )}

        {/* Actions */}
        <div className="mt-5 flex flex-wrap items-center gap-3">
          <button
            id="sdp-upload-submit"
            onClick={handleUpload}
            disabled={!submissionAllowed || uploading}
            className="inline-flex items-center gap-2 rounded-lg bg-indigo-600 px-5 py-2 text-sm font-medium text-white transition-colors hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {uploading ? (
              <>
                <RefreshCw size={14} className="animate-spin" />
                Submitting to SDP…
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
              className="inline-flex items-center gap-2 rounded-lg border border-slate-300 px-4 py-2 text-sm font-medium text-slate-600 transition-colors hover:bg-slate-50"
            >
              <X size={14} />
              Clear
            </button>
          )}
          {!submissionAllowed &&
            validation.kind === "ok" &&
            summary &&
            (summary.headerErrors.length > 0 || summary.invalidRows > 0) && (
              <span className="inline-flex items-center gap-1.5 rounded-md bg-amber-100 px-2.5 py-1 text-xs font-medium text-amber-800">
                <Info size={12} />
                Fix validation issues before submission
              </span>
            )}
        </div>
      </section>

      {/* ── Recent SDP Disbursements ─────────────────────────────────────── */}
      <section className="overflow-hidden rounded-2xl border border-slate-200 bg-white shadow-sm">
        <div className="flex items-center justify-between border-b border-slate-200 px-6 py-4">
          <h2 className="font-semibold text-slate-800">
            Recent SDP Disbursements
          </h2>
          <button
            onClick={refreshSdp}
            className="inline-flex items-center gap-1.5 rounded-lg border border-slate-300 px-3 py-1.5 text-xs text-slate-600 hover:bg-slate-50"
          >
            <RefreshCw size={12} />
            Refresh
          </button>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="bg-slate-50 text-xs uppercase text-slate-500">
              <tr>
                {["Name", "Status", "Asset", "Payments", "Amount", "Created"].map(
                  (h) => (
                    <th
                      key={h}
                      className="px-6 py-3 text-left font-semibold"
                    >
                      {h}
                    </th>
                  ),
                )}
                <th className="px-6 py-3 text-right font-semibold">Action</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100">
              {!disbursements ? (
                Array.from({ length: 4 }).map((_, i) => (
                  <tr key={i}>
                    {Array.from({ length: 7 }).map((_, j) => (
                      <td key={j} className="px-6 py-3">
                        <div className="h-4 animate-pulse rounded bg-slate-100" />
                      </td>
                    ))}
                  </tr>
                ))
              ) : disbursements.length === 0 ? (
                <tr>
                  <td
                    colSpan={7}
                    className="px-6 py-10 text-center text-slate-400"
                  >
                    No SDP disbursements yet — upload a CSV above to create one.
                  </td>
                </tr>
              ) : (
                disbursements.map((d) => (
                  <tr key={d.id} className="hover:bg-slate-50">
                    <td className="px-6 py-3">
                      <div className="font-medium text-slate-800">{d.name}</div>
                      <div className="font-mono text-[11px] text-slate-500">
                        {d.id.slice(0, 12)}…
                      </div>
                    </td>
                    <td className="px-6 py-3">
                      <StatusBadge status={d.status.toLowerCase()} />
                    </td>
                    <td className="px-6 py-3 text-slate-700">
                      {d.asset_code}
                      {d.asset_issuer && (
                        <span className="ml-1 font-mono text-[11px] text-slate-400">
                          {d.asset_issuer.slice(0, 6)}…
                        </span>
                      )}
                    </td>
                    <td className="px-6 py-3 text-slate-700">
                      <div>
                        <span className="font-medium">
                          {d.successful_payments}
                        </span>{" "}
                        <span className="text-slate-400">ok</span>
                      </div>
                      <div className="text-[11px] text-slate-500">
                        {d.total_payments} total · {d.failed_payments} failed ·{" "}
                        {d.cancelled_payments} cancelled
                      </div>
                    </td>
                    <td className="px-6 py-3 text-slate-700">
                      <div className="font-medium">
                        {Number(d.disbursed_amount).toLocaleString()}
                      </div>
                      <div className="text-[11px] text-slate-500">
                        of {Number(d.total_amount).toLocaleString()}
                      </div>
                    </td>
                    <td className="px-6 py-3 whitespace-nowrap text-slate-600">
                      {format(new Date(d.created_at), "MMM d, yyyy HH:mm")}
                    </td>
                    <td className="px-6 py-3 text-right">
                      <button
                        onClick={() => setLogsForId(d.id)}
                        className="rounded-md border border-slate-300 px-2.5 py-1 text-xs font-medium text-slate-600 hover:bg-slate-50"
                      >
                        Logs
                      </button>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      {logsForId && (
        <LogModal
          disbursementId={logsForId}
          onClose={() => setLogsForId(null)}
        />
      )}
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Small stat + banner components (kept local to this file — no need to hoist)
// ──────────────────────────────────────────────────────────────────────────────

function StatTile({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: "slate" | "emerald" | "red" | "indigo";
}) {
  const toneClasses = {
    slate: "border-slate-200 bg-white",
    emerald: "border-emerald-200 bg-emerald-50",
    red: "border-red-200 bg-red-50",
    indigo: "border-indigo-200 bg-indigo-50",
  }[tone];
  const valueClasses = {
    slate: "text-slate-800",
    emerald: "text-emerald-700",
    red: "text-red-700",
    indigo: "text-indigo-700",
  }[tone];
  return (
    <div className={`rounded-xl border p-3 ${toneClasses}`}>
      <div className="text-[11px] font-medium uppercase tracking-wide text-slate-500">
        {label}
      </div>
      <div className={`mt-1 text-2xl font-bold ${valueClasses}`}>{value}</div>
    </div>
  );
}

function ProgressRing({
  percent,
  size = 120,
  strokeWidth = 10,
  trackColor = "#e2e8f0",
  progressColor = "#4f46e5",
  label,
  sublabel,
}: {
  percent: number;
  size?: number;
  strokeWidth?: number;
  trackColor?: string;
  progressColor?: string;
  label?: string;
  sublabel?: string;
}) {
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const clamped = Math.max(0, Math.min(1, percent));
  const offset = circumference * (1 - clamped);

  return (
    <div className="flex flex-col items-center gap-1">
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke={trackColor}
          strokeWidth={strokeWidth}
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke={progressColor}
          strokeWidth={strokeWidth}
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={offset}
          transform={`rotate(-90 ${size / 2} ${size / 2})`}
          className="transition-all duration-700 ease-out"
        />
        <text
          x={size / 2}
          y={size / 2}
          textAnchor="middle"
          dominantBaseline="central"
          className="fill-slate-900 text-sm font-bold"
          style={{ fontSize: size * 0.22 }}
        >
          {Math.round(clamped * 100)}%
        </text>
      </svg>
      {label && (
        <p className="text-xs font-medium text-slate-700">{label}</p>
      )}
      {sublabel && (
        <p className="text-[11px] text-slate-500">{sublabel}</p>
      )}
    </div>
  );
}

function Banner({
  tone,
  icon,
  title,
  lines,
}: {
  tone: "success" | "warning" | "error" | "info";
  icon: React.ReactNode;
  title: string;
  lines: string[];
}) {
  const toneClasses = {
    success: "border-emerald-200 bg-emerald-50 text-emerald-800",
    warning: "border-amber-200 bg-amber-50 text-amber-800",
    error: "border-red-200 bg-red-50 text-red-800",
    info: "border-indigo-200 bg-indigo-50 text-indigo-800",
  }[tone];
  return (
    <div className={`flex items-start gap-2 rounded-lg border px-4 py-3 text-sm ${toneClasses}`}>
      <span className="mt-0.5 shrink-0">{icon}</span>
      <div>
        <p className="font-semibold">{title}</p>
        {lines.length > 0 && (
          <ul className="mt-1 list-disc space-y-0.5 pl-4 text-sm opacity-90">
            {lines.map((l, idx) => (
              <li key={idx}>{l}</li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function fileBasename(name: string): string {
  const idx = name.lastIndexOf(".");
  return idx > 0 ? name.slice(0, idx) : name;
}

// ──────────────────────────────────────────────────────────────────────────────
// Main page
// ──────────────────────────────────────────────────────────────────────────────

export default function PayoutsPage() {
  const [activeTab, setActiveTab] = useState<PayoutTab>("history");
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
  } = usePolling<Payout>(() => api.payoutHistory(20, 0) as any, 30000);
  const payouts = (payoutData as any)?.payouts ?? [];

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
      form.reset();
      await refreshPayouts();
    } catch {
      // errors show as alert banner in history area
    }
  };

  const TABS: { id: PayoutTab; label: string }[] = [
    { id: "history", label: "Payout History" },
    { id: "batch", label: "Batch Disbursements" },
    { id: "sdp", label: "SDP CSV Upload" },
  ];

  return (
    <div className="mx-auto max-w-7xl px-4 py-8 sm:px-6 lg:px-8">
      <div className="mb-8 flex flex-col md:flex-row md:items-center md:justify-between">
        <div>
          <h1 className="text-2xl font-bold text-slate-900">Payouts</h1>
          <p className="mt-1 text-sm text-slate-500">
            Manage bulk disbursement batches, payout history, and SDP CSV
            uploads.
          </p>
        </div>
      </div>

      {/* ── Tab nav ─────────────────────────────────────────────────────── */}
      <div className="mb-6 border-b border-slate-200">
        <nav className="-mb-px flex space-x-8 overflow-x-auto">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              onClick={() => {
                setActiveTab(tab.id);
                setSelectedBatch(null);
              }}
              className={`whitespace-nowrap border-b-2 px-1 py-4 text-sm font-medium transition-colors ${
                activeTab === tab.id
                  ? "border-indigo-500 text-indigo-600"
                  : "border-transparent text-slate-500 hover:text-slate-700"
              }`}
            >
              {tab.label}
            </button>
          ))}
        </nav>
      </div>

      {/* ── Payout History ──────────────────────────────────────────────── */}
      {activeTab === "history" && (
        <div className="space-y-6">
          <div className="max-w-lg rounded-xl border border-slate-200 bg-white p-5 shadow-sm">
            <h2 className="mb-4 font-semibold text-slate-800">
              Request Payout
            </h2>
            <form onSubmit={handleSubmitPayout} className="space-y-3">
              <div className="flex gap-2">
                <input
                  required
                  type="number"
                  min="0.01"
                  step="0.01"
                  name="amount"
                  placeholder="Amount"
                  className="w-full flex-1 rounded-lg border border-slate-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                />
                <select
                  name="asset"
                  className="rounded-lg border border-slate-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
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
                className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
              <input
                required
                name="anchorId"
                placeholder="Anchor ID"
                className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
              <button
                type="submit"
                className="w-full rounded-lg bg-indigo-600 py-2 text-sm font-medium text-white transition-colors hover:bg-indigo-700 disabled:opacity-50"
              >
                Request Payout
              </button>
            </form>
          </div>

          {payoutError && (
            <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">
              {payoutError}
            </div>
          )}
          <div className="overflow-hidden rounded-xl border border-slate-200 bg-white shadow-sm">
            <div className="border-b border-slate-200 px-6 py-4">
              <h3 className="font-semibold text-slate-800">Recent Payouts</h3>
            </div>
            <table className="w-full text-sm">
              <thead className="border-b border-slate-200 bg-slate-50">
                <tr>
                  {[
                    "ID",
                    "Date",
                    "Amount",
                    "Asset",
                    "Status",
                    "Anchor",
                  ].map((h) => (
                    <th
                      key={h}
                      className="px-6 py-3 text-left text-xs font-semibold uppercase tracking-wide text-slate-500"
                    >
                      {h}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {payoutLoading && payouts.length === 0 ? (
                  skeletonRows(5, 6)
                ) : payouts.length === 0 ? (
                  <tr>
                    <td
                      colSpan={6}
                      className="px-6 py-10 text-center text-slate-400"
                    >
                      No payouts yet
                    </td>
                  </tr>
                ) : (
                  payouts.map((p: Payout) => (
                    <tr key={p.id} className="hover:bg-slate-50">
                      <td className="px-6 py-3 font-mono text-xs text-slate-500">
                        {p.id.slice(0, 8)}…
                      </td>
                      <td className="whitespace-nowrap px-6 py-3 text-slate-600">
                        {format(new Date(p.createdAt), "MMM d, yyyy")}
                      </td>
                      <td className="px-6 py-3 font-medium">
                        {(Number(p.amount) / 1_000_000).toFixed(2)}
                      </td>
                      <td className="px-6 py-3">{p.asset}</td>
                      <td className="px-6 py-3">
                        <StatusBadge status={p.status} />
                      </td>
                      <td className="px-6 py-3 text-xs text-slate-400">
                        {p.anchorId}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* ── Batch Disbursements ─────────────────────────────────────────── */}
      {activeTab === "batch" && (
        <div className="space-y-6">
          {(() => {
            const totalRecipients = batches.reduce(
              (sum, b) => sum + (b.total_recipients || 0),
              0,
            );
            const totalSucceeded = batches.reduce(
              (sum, b) => sum + (b.succeeded_count || 0),
              0,
            );
            const successRate = totalRecipients > 0 ? totalSucceeded / totalRecipients : 0;

            return (
              <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
                <div className="rounded-xl border border-slate-200 bg-white p-5 shadow-sm">
                  <p className="text-sm text-slate-500">Total Batches</p>
                  <p className="mt-1 text-3xl font-bold text-slate-900">
                    {batches.length}
                  </p>
                </div>
                <div className="rounded-xl border border-slate-200 bg-white p-5 shadow-sm">
                  <p className="text-sm text-slate-500">Total Recipients</p>
                  <p className="mt-1 text-3xl font-bold text-slate-900">
                    {totalRecipients}
                  </p>
                </div>
                <div className="rounded-xl border border-slate-200 bg-white p-5 shadow-sm">
                  <p className="text-sm text-slate-500">Total Volume (USDC)</p>
                  <p className="mt-1 text-3xl font-bold text-slate-900">
                    {(
                      batches.reduce(
                        (sum, b) => sum + (b.total_amount || 0),
                        0,
                      ) / 1_000_000
                    ).toLocaleString()}
                  </p>
                </div>
                <div className="flex flex-col items-center justify-center rounded-xl border border-slate-200 bg-white p-5 shadow-sm">
                  <ProgressRing
                    percent={successRate}
                    size={110}
                    strokeWidth={10}
                    progressColor={successRate === 1 ? "#059669" : "#4f46e5"}
                    label="Success Rate"
                    sublabel={`${totalSucceeded} of ${totalRecipients} recipients`}
                  />
                </div>
              </div>
            );
          })()}

          {batchError && (
            <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">
              {batchError}
            </div>
          )}
          <div className="overflow-hidden rounded-xl border border-slate-200 bg-white shadow-sm">
            <div className="border-b border-slate-200 px-6 py-4">
              <h3 className="font-semibold text-slate-800">
                Batch Disbursement History
              </h3>
            </div>
            <table className="w-full text-sm">
              <thead className="border-b border-slate-200 bg-slate-50">
                <tr>
                  {[
                    "Batch ID",
                    "Date",
                    "Recipients",
                    "Total",
                    "Status",
                    "Results",
                  ].map((h) => (
                    <th
                      key={h}
                      className="px-6 py-3 text-left text-xs font-semibold uppercase tracking-wide text-slate-500"
                    >
                      {h}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {batchLoading && batches.length === 0 ? (
                  skeletonRows(5, 6)
                ) : batches.length === 0 ? (
                  <tr>
                    <td
                      colSpan={6}
                      className="px-6 py-10 text-center text-slate-400"
                    >
                      No batch disbursements yet
                    </td>
                  </tr>
                ) : (
                  batches.map((batch) => (
                    <tr
                      key={batch.id}
                      className="cursor-pointer hover:bg-slate-50"
                      onClick={() => setSelectedBatch(batch.id)}
                    >
                      <td className="px-6 py-3 font-mono text-xs text-slate-500">
                        {batch.id.slice(0, 8)}…
                      </td>
                      <td className="whitespace-nowrap px-6 py-3 text-slate-600">
                        {format(
                          new Date(batch.created_at),
                          "MMM d, yyyy HH:mm",
                        )}
                      </td>
                      <td className="px-6 py-3 font-medium">
                        {batch.total_recipients}
                      </td>
                      <td className="px-6 py-3 font-medium">
                        {(batch.total_amount / 1_000_000).toLocaleString()}{" "}
                        {batch.currency}
                      </td>
                      <td className="px-6 py-3">
                        <StatusBadge status={batch.status} />
                      </td>
                      <td className="px-6 py-3 text-xs text-slate-500">
                        {batch.succeeded_count} succeeded, {batch.failed_count}{" "}
                        failed
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>

          {selectedBatch && batchDetails && (
            <BatchDetailsModal
              batch={batchDetails.batch}
              recipients={batchDetails.recipients}
              onClose={() => setSelectedBatch(null)}
            />
          )}
        </div>
      )}

      {/* ── SDP CSV Upload Tab (actual implementation of the task) ──────── */}
      {activeTab === "sdp" && <SdpDisbursementTab />}
    </div>
  );
}

function skeletonRows(rows: number, cols: number) {
  return Array.from({ length: rows }).map((_, i) => (
    <tr key={i}>
      {Array.from({ length: cols }).map((_, j) => (
        <td key={j} className="px-6 py-3">
          <div className="h-4 animate-pulse rounded bg-slate-100" />
        </td>
      ))}
    </tr>
  ));
}

function BatchDetailsModal({
  batch,
  recipients,
  onClose,
}: {
  batch: BatchPayout;
  recipients: BatchRecipient[];
  onClose: () => void;
}) {
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 p-4"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="flex max-h-[85vh] w-full max-w-4xl flex-col overflow-hidden rounded-2xl bg-white shadow-2xl">
        <div className="flex items-center justify-between border-b border-slate-200 px-6 py-4">
          <h3 className="font-semibold text-slate-800">Batch Details</h3>
          <button
            onClick={onClose}
            className="rounded-lg p-1.5 text-slate-400 transition hover:bg-slate-100 hover:text-slate-600"
            aria-label="Close"
          >
            <X size={18} />
          </button>
        </div>
        <div className="flex-1 overflow-y-auto p-6">
          <div className="grid grid-cols-2 gap-4 md:grid-cols-3">
            <InfoCell label="Batch ID" value={batch.id} mono />
            <InfoCell label="Status">
              <StatusBadge status={batch.status} />
            </InfoCell>
            <InfoCell label="Currency" value={batch.currency} />
            <InfoCell
              label="Total Recipients"
              value={batch.total_recipients.toString()}
            />
            <InfoCell
              label="Total Amount"
              value={`${(batch.total_amount / 1_000_000).toLocaleString()} ${batch.currency}`}
            />
            <InfoCell
              label="Created"
              value={format(new Date(batch.created_at), "MMM d, yyyy HH:mm")}
            />
            <InfoCell
              label="Succeeded"
              value={batch.succeeded_count.toString()}
              tone="emerald"
            />
            <InfoCell
              label="Failed"
              value={batch.failed_count.toString()}
              tone="red"
            />
            {batch.started_at && (
              <InfoCell
                label="Started"
                value={format(new Date(batch.started_at), "MMM d, HH:mm")}
              />
            )}
            {batch.completed_at && (
              <InfoCell
                label="Completed"
                value={format(new Date(batch.completed_at), "MMM d, HH:mm")}
              />
            )}
          </div>

          {recipients.length > 0 && (
            <div className="mt-6 border-t border-slate-200 pt-6">
              <h4 className="mb-3 font-semibold text-slate-800">
                Recipients ({recipients.length})
              </h4>
              <div className="overflow-x-auto rounded-xl border border-slate-200">
                <table className="w-full text-sm">
                  <thead className="bg-slate-50">
                    <tr>
                      {[
                        "Status",
                        "Amount",
                        "Destination",
                        "Attempts",
                        "Tx Hash",
                      ].map((h) => (
                        <th
                          key={h}
                          className="px-4 py-2 text-left text-xs font-medium uppercase tracking-wide text-slate-500"
                        >
                          {h}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-100">
                    {recipients.map((r) => (
                      <tr key={r.id}>
                        <td className="px-4 py-2">
                          <StatusBadge status={r.status} />
                        </td>
                        <td className="px-4 py-2 font-medium">
                          {(r.amount / 1_000_000).toLocaleString()}
                        </td>
                        <td className="break-all px-4 py-2 font-mono text-xs text-slate-600">
                          {r.destination_address || r.user_id || "N/A"}
                        </td>
                        <td className="px-4 py-2 text-slate-600">
                          {r.attempt_count}
                        </td>
                        <td className="break-all px-4 py-2 font-mono text-xs text-slate-500">
                          {r.tx_hash ? `${r.tx_hash.slice(0, 16)}…` : "Pending"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>
        <div className="flex justify-end border-t border-slate-200 px-6 py-4">
          <button
            onClick={onClose}
            className="rounded-lg bg-indigo-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-indigo-700"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

function InfoCell({
  label,
  value,
  children,
  mono,
  tone,
}: {
  label: string;
  value?: string;
  children?: React.ReactNode;
  mono?: boolean;
  tone?: "emerald" | "red";
}) {
  const valueClasses =
    tone === "emerald"
      ? "text-emerald-700"
      : tone === "red"
        ? "text-red-700"
        : "text-slate-800";
  return (
    <div className="rounded-lg bg-slate-50 p-3">
      <p className="text-xs font-medium uppercase tracking-wide text-slate-500">
        {label}
      </p>
      <div
        className={`mt-1 ${mono ? "font-mono text-xs" : "font-medium"} ${valueClasses}`}
      >
        {children ?? value ?? "—"}
      </div>
    </div>
  );
}
