"use client";

import { useAddressCopy, truncateAddress } from "@/lib/utils";
import { Check, Copy } from "lucide-react";

interface TruncatedAddressProps {
  /** Full 56-character Stellar public address (G...). */
  address: string;
  /** Optional custom CSS class name. */
  className?: string;
  /** Number of leading characters to show (default: 1, e.g. 'G'). */
  head?: number;
  /** Number of trailing characters to show (default: 4, e.g. 'ABCD'). */
  tail?: number;
  /** Whether to show the copy icon alongside the address (default: true). */
  showCopyIcon?: boolean;
}

/**
 * Truncated Stellar Address with click-to-copy, checkmark feedback,
 * and a 2-second 'Copied!' tooltip message (#796).
 */
export default function TruncatedAddress({
  address,
  className = "",
  head = 1,
  tail = 4,
  showCopyIcon = true,
}: TruncatedAddressProps) {
  const { copied, copy } = useAddressCopy(2000);
  const displayAddress = truncateAddress(address, head, tail);

  const handleCopy = async (e: React.MouseEvent | React.KeyboardEvent) => {
    e.stopPropagation();
    await copy(address);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      handleCopy(e);
    }
  };

  return (
    <span className="relative inline-flex items-center">
      <button
        type="button"
        onClick={handleCopy}
        onKeyDown={handleKeyDown}
        data-testid="truncated-address-btn"
        aria-label={`Copy address ${address}`}
        title={`Click to copy: ${address}`}
        className={`group inline-flex items-center gap-1.5 rounded-md px-1.5 py-0.5 font-mono text-xs text-slate-700 transition-colors hover:bg-slate-100 hover:text-indigo-600 focus:outline-none focus:ring-2 focus:ring-indigo-500/50 dark:text-slate-300 dark:hover:bg-slate-800 dark:hover:text-indigo-400 ${className}`}
      >
        <span className="select-all font-mono">{displayAddress}</span>
        {showCopyIcon && (
          <span className="shrink-0 text-slate-400 group-hover:text-indigo-600 dark:group-hover:text-indigo-400">
            {copied ? (
              <Check
                size={13}
                className="text-emerald-500"
                data-testid="copy-checkmark-icon"
              />
            ) : (
              <Copy size={13} data-testid="copy-action-icon" />
            )}
          </span>
        )}
      </button>

      {/* 2-second 'Copied!' tooltip feedback */}
      {copied && (
        <span
          role="status"
          data-testid="copied-tooltip"
          className="absolute -top-7 left-1/2 -translate-x-1/2 z-30 rounded bg-slate-900 px-2 py-0.5 text-[11px] font-medium text-white shadow-md animate-fade-in pointer-events-none whitespace-nowrap dark:bg-slate-800 border border-slate-700/50"
        >
          Copied!
          <span className="absolute left-1/2 top-full -translate-x-1/2 border-4 border-transparent border-t-slate-900 dark:border-t-slate-800" />
        </span>
      )}
    </span>
  );
}
