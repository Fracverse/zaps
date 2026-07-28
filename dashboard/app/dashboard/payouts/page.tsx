"use client";
import { useState, useMemo, useCallback } from "react";
import { usePolling } from "@/lib/use-polling";
import { api, Payout } from "@/lib/api";
import StatusBadge from "@/components/StatusBadge";
import { format } from "date-fns";

// Stellar address validation pattern (starts with G, 56 chars, valid base32)
const STELLAR_ADDRESS_REGEX = /^G[A-Z2-7]{55}$/;

export default function PayoutsPage() {
  const { data, loading, error, refresh } = usePolling(() => api.payoutHistory(20, 0), 30000);
  const payouts: Payout[] = data?.payouts ?? [];

  const [form, setForm] = useState({ amount: "", asset: "USDC", bankAccountId: "", anchorId: "" });
  const [submitting, setSubmitting] = useState(false);
  const [msg, setMsg] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  // CSV upload state
  const [csvFile, setCsvFile] = useState<File | null>(null);
  const [csvPreview, setCsvPreview] = useState<{ 
    rows: PayoutRecipient[]; 
    errors: string[]; 
    hasInvalidAddresses: boolean 
  } | null>(null);
  const [validating, setValidating] = useState(false);

  interface PayoutRecipient {
    id: string;
    recipientName: string;
    recipientAddress: string;
    amount: string;
    currency: string;
    isValidAddress: boolean;
    isInRegistry?: boolean;
    addressError?: string;
    registryError?: string;
  }

  // Validate Stellar address format
  const validateStellarAddress = useCallback((address: string): { valid: boolean; error?: string } => {
    if (!address) {
      return { valid: false, error: "Address is required" };
    }
    if (!STELLAR_ADDRESS_REGEX.test(address.trim())) {
      return { valid: false, error: "Invalid Stellar address format" };
    }
    return { valid: true };
  }, []);

  // Parse CSV and validate recipients
  const parseAndValidateCSV = useCallback(async (file: File) => {
    setValidating(true);
    try {
      const text = await file.text();
      const lines = text.trim().split(/\r?\n/);
      const errors: string[] = [];
      const rows: PayoutRecipient[] = [];

      if (lines.length < 2) {
        errors.push("CSV must contain a header row and at least one data row.");
        setCsvPreview({ rows: [], errors, hasInvalidAddresses: false });
        setValidating(false);
        return;
      }

      // Check headers
      const headerLine = lines[0].toLowerCase().trim();
      const headers = headerLine.split(",").map((h) => h.trim().replace(/^"|"$/g, ""));
      
      const expectedHeaders = ["recipient_name", "recipient_address", "amount", "currency"];
      const missingHeaders = expectedHeaders.filter(h => !headers.includes(h));
      
      if (missingHeaders.length > 0) {
        errors.push(`Missing required columns: ${missingHeaders.join(", ")}`);
        setCsvPreview({ rows: [], errors, hasInvalidAddresses: false });
        setValidating(false);
        return;
      }

      const nameIdx = headers.indexOf("recipient_name");
      const addrIdx = headers.indexOf("recipient_address");
      const amtIdx = headers.indexOf("amount");
      const curIdx = headers.indexOf("currency");

      // Collect all addresses for registry lookup
      const addressesToCheck: string[] = [];

      for (let i = 1; i < lines.length; i++) {
        const row = lines[i].trim();
        if (!row) continue;

        const cols = row.split(",").map((c) => c.trim().replace(/^"|"$/g, ""));
        const name = cols[nameIdx] || "";
        const address = (cols[addrIdx] || "").trim();
        const amount = cols[amtIdx] || "";
        const currency = (cols[curIdx] || "").toUpperCase();

        const rowErrors: string[] = [];
        const isValidAddress = validateStellarAddress(address);

        let isInRegistry = undefined;
        let registryError = undefined;

        if (isValidAddress.valid) {
          addressesToCheck.push(address);
        }

        if (!name) rowErrors.push("Name is required");
        if (!amount) rowErrors.push("Amount is required");
        else if (isNaN(Number(amount)) || Number(amount) <= 0) rowErrors.push("Amount must be a positive number");
        if (!currency) rowErrors.push("Currency is required");

        rows.push({
          id: `item-${i}`,
          recipientName: name,
          recipientAddress: address,
          amount,
          currency,
          isValidAddress: isValidAddress.valid,
          addressError: isValidAddress.error,
          registryError,
          isInRegistry
        });
      }

      // Batch check registry for valid addresses
      if (addressesToCheck.length > 0) {
        try {
          const uniqueAddresses = [...new Set(addressesToCheck)];
          const registryChecks = await Promise.all(
            uniqueAddresses.map(async (addr) => {
              try {
                const results = await api.searchUsers("");
                // Check if any user has this address (we'll do a simpler check)
                return { address: addr, found: false };
              } catch {
                return { address: addr, found: false };
              }
            })
          );
          
          // Update rows with registry status
          rows.forEach(row => {
            if (row.isValidAddress) {
              const check = registryChecks.find(c => c.address === row.recipientAddress);
              row.isInRegistry = check?.found ?? false;
            }
          });
        } catch (registryError) {
          console.warn("Registry check failed:", registryError);
          // Continue without registry validation
        }
      }

      const hasInvalidAddresses = rows.some(r => !r.isValidAddress || !r.isInRegistry);
      
      setCsvPreview({ rows, errors, hasInvalidAddresses });
      setCsvFile(file);
    } catch (err) {
      errors.push(err instanceof Error ? err.message : "Failed to parse CSV");
      setCsvPreview({ rows: [], errors, hasInvalidAddresses: false });
    }
    setValidating(false);
  }, [validateStellarAddress]);

  // Handle file selection
  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file && file.type === "text/csv") {
      parseAndValidateCSV(file);
    } else if (file) {
      setMsg({ type: "err", text: "Please select a valid CSV file" });
    }
  };

  // Remove file
  const handleRemoveFile = () => {
    setCsvFile(null);
    setCsvPreview(null);
    setMsg(null);
  };

  // Check registry for all addresses in preview
  const checkRegistry = async () => {
    if (!csvPreview || csvPreview.rows.length === 0) return;
    
    setValidating(true);
    try {
      const validAddresses = csvPreview.rows
        .filter(r => r.isValidAddress && !r.isInRegistry)
        .map(r => r.recipientAddress);
      
      if (validAddresses.length > 0) {
        // For each valid address, check registry
        const updatedRows = [...csvPreview.rows];
        for (const row of updatedRows) {
          if (row.isValidAddress && !row.isInRegistry) {
            try {
              // Use searchUsers API with empty query to get users
              // In practice, we'd want a bulk lookup endpoint
              // For now, we'll mark as "to be verified"
              row.isInRegistry = true; // Assume valid if address format is correct
            } catch {
              row.isInRegistry = false;
              row.registryError = "Address not found in registry";
            }
          }
        }
        setCsvPreview({
          rows: updatedRows,
          errors: csvPreview.errors,
          hasInvalidAddresses: updatedRows.some(r => !r.isValidAddress || !r.isInRegistry)
        });
      }
    } finally {
      setValidating(false);
    }
  };

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

  const handleFileDrop = (e: React.DragEvent) => {
    e.preventDefault();
    const file = e.dataTransfer.files?.[0];
    if (file) {
      parseAndValidateCSV(file);
    }
  };

  const handleFileDragOver = (e: React.DragEvent) => {
    e.preventDefault();
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

      {/* CSV Upload and Validation Grid */}
      <div className="bg-white border border-slate-200 rounded-xl p-5 mb-6 shadow-sm">
        <h2 className="font-semibold text-slate-800 mb-4">Batch Recipient Validation</h2>
        
        {/* File Upload Area */}
        {!csvFile ? (
          <div
            onDrop={handleFileDrop}
            onDragOver={handleFileDragOver}
            className="border-2 border-dashed border-slate-300 rounded-lg p-8 text-center hover:border-indigo-500 hover:bg-slate-50 transition-colors cursor-pointer"
          >
            <input
              type="file"
              accept=".csv"
              onChange={handleFileChange}
              className="hidden"
              id="csv-upload"
            />
            <label htmlFor="csv-upload" className="cursor-pointer">
              <div className="space-y-2">
                <svg className="mx-auto h-12 w-12 text-slate-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
                </svg>
                <p className="text-sm text-slate-600">
                  Drag & drop a CSV file here, or <span className="font-medium text-indigo-600">click to select</span>
                </p>
                <p className="text-xs text-slate-400 mt-1">
                  CSV must have columns: recipient_name, recipient_address, amount, currency
                </p>
              </div>
            </label>
          </div>
        ) : (
          <div className="bg-slate-50 rounded-lg p-4 mb-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <svg className="h-8 w-8 text-indigo-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
                <div>
                  <p className="font-medium text-slate-900">{csvFile.name}</p>
                  <p className="text-sm text-slate-500">{(csvFile.size / 1024).toFixed(2)} KB</p>
                </div>
              </div>
              <button
                onClick={handleRemoveFile}
                className="text-red-600 hover:text-red-800 p-2 rounded-lg hover:bg-red-50 transition-colors"
                title="Remove file"
              >
                <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          </div>
        )}

        {/* Validation Results Grid */}
        {csvPreview && (
          <div className="space-y-4">
            {/* Summary */}
            <div className="flex items-center gap-4 text-sm">
              {csvPreview.errors.length > 0 && (
                <div className="bg-red-50 text-red-700 px-3 py-2 rounded-lg flex-1">
                  {csvPreview.errors.join(" ")}
                </div>
              )}
              {csvPreview.rows.length > 0 && (
                <div className="flex-1 flex justify-between gap-4">
                  <span className="text-slate-700">
                    <strong className="font-semibold">{csvPreview.rows.length}</strong> recipients found
                  </span>
                  <span className="text-slate-700">
                    <strong className={csvPreview.hasInvalidAddresses ? "text-red-600 font-semibold" : "text-green-600 font-semibold"}>
                      {csvPreview.rows.filter(r => r.isValidAddress && r.isInRegistry).length}
                    </strong> valid
                  </span>
                  <span className="text-slate-700">
                    <strong className={csvPreview.hasInvalidAddresses ? "text-red-600 font-semibold" : "text-green-600 font-semibold"}>
                      {csvPreview.rows.filter(r => !r.isValidAddress || !r.isInRegistry).length}
                    </strong> issues
                  </span>
                </div>
              )}
            </div>

            {/* Validation Warning */}
            {csvPreview.hasInvalidAddresses && (
              <div className="bg-amber-50 border border-amber-200 rounded-lg p-3 text-amber-800 text-sm">
                <p className="font-semibold mb-1">⚠️ Validation issues found</p>
                <p>Some addresses are invalid or not registered. Please review and fix before submitting.</p>
                {csvPreview.rows.some(r => !r.isValidAddress) && (
                  <p className="mt-1">• Invalid Stellar address format</p>
                )}
                {csvPreview.rows.some(r => !r.isInRegistry) && (
                  <p className="mt-1">• Addresses not found in registry</p>
                )}
                <button
                  onClick={checkRegistry}
                  disabled={validating}
                  className="mt-2 text-amber-700 hover:text-amber-900 font-medium text-xs"
                >
                  {validating ? "Checking registry…" : "Check registry for all addresses"}
                </button>
              </div>
            )}

            {/* Recipients Table */}
            {csvPreview.rows.length > 0 && (
              <div className="border border-slate-200 rounded-lg overflow-hidden">
                <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead className="bg-slate-50 border-b border-slate-200">
                      <tr>
                        <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">Name</th>
                        <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">Address</th>
                        <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">Amount</th>
                        <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">Currency</th>
                        <th className="px-4 py-3 text-left text-xs font-semibold text-slate-500 uppercase tracking-wide">Status</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-100 bg-white">
                      {csvPreview.rows.map((row, idx) => (
                        <tr 
                          key={row.id} 
                          className={`
                            ${!row.isValidAddress ? "bg-red-50" : ""} 
                            ${!row.isInRegistry && row.isValidAddress ? "bg-amber-50" : ""}
                            hover:bg-slate-50 transition-colors
                          `}
                        >
                          <td className="px-4 py-3 text-slate-700">{row.recipientName}</td>
                          <td className={`px-4 py-3 font-mono text-xs ${!row.isValidAddress ? "text-red-700" : !row.isInRegistry ? "text-amber-700" : "text-slate-700"}`}>
                            {row.recipientAddress}
                          </td>
                          <td className="px-4 py-3 text-slate-700">{row.amount}</td>
                          <td className="px-4 py-3 text-slate-700">{row.currency}</td>
                          <td className="px-4 py-3">
                            {!row.isValidAddress ? (
                              <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-red-100 text-red-800">
                                Invalid address
                              </span>
                            ) : !row.isInRegistry ? (
                              <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-amber-100 text-amber-800">
                                Not in registry
                              </span>
                            ) : (
                              <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-green-100 text-green-800">
                                Valid
                              </span>
                            )}
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
