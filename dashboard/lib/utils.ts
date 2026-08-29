import { useCallback, useEffect, useRef, useState } from "react";
import { Transaction } from "@/lib/api";

/** Convert transactions array to CSV string */
export function toCSV(rows: Transaction[]): string {
  const headers = ["id", "created_at", "from_address", "send_asset", "send_amount", "receive_amount", "status", "memo"];
  const lines = [
    headers.join(","),
    ...rows.map((r) =>
      [r.id, r.created_at, r.from_address, r.send_asset, r.send_amount, r.receive_amount ?? "", r.status, r.memo ?? ""]
        .map((v) => `"${String(v).replace(/"/g, '""')}"`)
        .join(",")
    ),
  ];
  return lines.join("\n");
}

export function downloadBlob(content: string, filename: string, mime: string) {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export function fmtAmount(amount: number, asset: string) {
  const val = amount / 1_000_000; // stroops → units
  return `${val.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 6 })} ${asset}`;
}

export function statusColor(status: string) {
  return {
    completed: "bg-green-100 text-green-800",
    pending: "bg-yellow-100 text-yellow-800",
    processing: "bg-blue-100 text-blue-800",
    retrying: "bg-blue-100 text-blue-800",
    dispatched: "bg-indigo-100 text-indigo-800",
    failed: "bg-red-100 text-red-800",
    refunded: "bg-gray-100 text-gray-700",
  }[status] ?? "bg-gray-100 text-gray-700";
}

// ──────────────────────────────────────────────────────────────────────────────
// Stellar Address Formatting & Clipboard Utilities (#796)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Truncates a Stellar public address into standard G...ABCD format (#796).
 *
 * @param address - Full 56-character Stellar public address (G...)
 * @param head - Leading character count (default: 1, returning 'G')
 * @param tail - Trailing character count (default: 4, returning 'ABCD')
 * @returns Truncated address string, e.g. "G...ABCD", or original string if too short.
 */
export function truncateAddress(
  address: string,
  head: number = 1,
  tail: number = 4,
): string {
  if (!address || typeof address !== "string") return "";
  if (address.length <= head + tail) return address;
  return `${address.slice(0, head)}...${address.slice(-tail)}`;
}

/**
 * Copy text to clipboard using the asynchronous Clipboard API with legacy fallback (#796).
 *
 * @param text - The text string to copy to the clipboard.
 * @returns Promise resolving to true if copy succeeded, false otherwise.
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  if (!text) return false;
  try {
    if (
      typeof navigator !== "undefined" &&
      navigator.clipboard &&
      typeof navigator.clipboard.writeText === "function"
    ) {
      await navigator.clipboard.writeText(text);
      return true;
    }
    // Fallback for non-secure contexts or legacy browsers
    if (typeof document !== "undefined") {
      const textarea = document.createElement("textarea");
      textarea.value = text;
      textarea.style.position = "fixed";
      textarea.style.opacity = "0";
      document.body.appendChild(textarea);
      textarea.select();
      const successful = document.execCommand("copy");
      document.body.removeChild(textarea);
      return successful;
    }
    return false;
  } catch {
    return false;
  }
}

/**
 * React hook to manage address copy-to-clipboard state with a 2-second feedback timer (#796).
 *
 * @param durationMs - Duration in milliseconds to maintain copied state (default: 2000 ms).
 * @returns Object containing `copied` boolean state and `copy` trigger function.
 */
export function useAddressCopy(durationMs: number = 2000) {
  const [copied, setCopied] = useState(false);
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);

  const copy = useCallback(
    async (text: string) => {
      const success = await copyToClipboard(text);
      if (success) {
        setCopied(true);
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => {
          setCopied(false);
        }, durationMs);
      }
      return success;
    },
    [durationMs],
  );

  useEffect(() => {
    return () => {
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, []);

  return { copied, copy };
}

