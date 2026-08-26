"use client";

import { Suspense, useCallback, useRef, useState } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { api, type UserSearchResult } from "@/lib/api";

/** Fields used to filter the dashboard transactions table in real time. */
export type TransactionFilterable = {
  from_address?: string;
  memo?: string | null;
  status?: string;
  sender_username?: string;
  receiver_username?: string;
  tx_hash?: string;
};

/**
 * Filter a table array by address, memo, or status (also matches related
 * username / hash fields so the same helper works on the social feed).
 */
export function filterTransactionRows<T extends TransactionFilterable>(
  rows: T[],
  query: string,
): T[] {
  const term = query.trim().toLowerCase();
  if (!term) return rows;
  return rows.filter((row) =>
    [row.from_address, row.memo, row.status, row.sender_username, row.receiver_username, row.tx_hash]
      .some((value) => (value ?? "").toLowerCase().includes(term)),
  );
}

export default function SearchBar() {
  return (
    <Suspense fallback={<SearchBarFallback />}>
      <SearchBarInner />
    </Suspense>
  );
}

function SearchBarFallback() {
  return (
    <input
      type="text"
      readOnly
      placeholder="Search address, memo, or status…"
      className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm
                 focus:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-200"
      aria-hidden="true"
    />
  );
}

function SearchBarInner() {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const [query, setQuery] = useState(searchParams.get("q") ?? "");
  const [results, setResults] = useState<UserSearchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [open, setOpen] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const syncQueryToUrl = useCallback(
    (value: string) => {
      if (typeof router.replace !== "function") return;
      const params = new URLSearchParams(searchParams.toString());
      if (value.trim()) params.set("q", value);
      else params.delete("q");
      const qs = params.toString();
      router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false });
    },
    [pathname, router, searchParams],
  );

  const debouncedSearch = useCallback((value: string) => {
    if (timerRef.current) clearTimeout(timerRef.current);
    if (value.trim().length < 2) {
      setResults([]);
      setOpen(false);
      return;
    }
    timerRef.current = setTimeout(async () => {
      setLoading(true);
      try {
        const data = await api.searchUsers(value.trim());
        setResults(data);
        setOpen(data.length > 0);
      } catch {
        setResults([]);
      } finally {
        setLoading(false);
      }
    }, 300);
  }, []);

  const handleSelect = (username: string) => {
    setQuery("");
    setOpen(false);
    router.push(`/dashboard/transactions?user=${encodeURIComponent(username)}`);
  };

  return (
    <div className="relative">
      <input
        type="text"
        value={query}
        onChange={(e) => {
          const value = e.target.value;
          setQuery(value);
          syncQueryToUrl(value);
          debouncedSearch(value);
        }}
        onFocus={() => results.length > 0 && setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 200)}
        placeholder="Search address, memo, or status…"
        className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm
                   focus:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-200"
        aria-label="Search users by username"
      />
      {loading && (
        <span className="absolute right-3 top-2.5 h-4 w-4 animate-spin rounded-full border-2 border-slate-300 border-t-indigo-600" />
      )}
      {open && (
        <ul className="absolute z-30 mt-1 max-h-60 w-full overflow-auto rounded-lg border border-slate-200 bg-white shadow-lg">
          {results.map((u) => (
            <li key={u.username}>
              <button
                type="button"
                onMouseDown={() => handleSelect(u.username)}
                className="flex w-full flex-col px-4 py-2.5 text-left text-sm hover:bg-indigo-50"
              >
                <span className="font-medium text-slate-900">{u.username}</span>
                <span className="text-xs text-slate-500 truncate">{u.public_key}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ── #795 Paginated table footer ────────────────────────────────────────────

/** Available page-size options across all paginated transaction tables. */
export const PAGE_SIZE_OPTIONS = [10, 25, 50, 100] as const;
export type PageSizeOption = (typeof PAGE_SIZE_OPTIONS)[number];

interface PaginationFooterProps {
  /** Total number of items in the filtered/full dataset. */
  total: number;
  /** Current zero-based page index. */
  page: number;
  /** Number of rows per page. */
  pageSize: PageSizeOption;
  /** Called when the user selects a different page size; resets to page 0. */
  onPageSizeChange: (size: PageSizeOption) => void;
  /** Called when the user clicks Prev / Next. */
  onPageChange: (page: number) => void;
}

/**
 * Reusable pagination footer for data tables.
 *
 * Renders:
 *  - an items-per-page selector (10 / 25 / 50 / 100)
 *  - current range and total count label
 *  - Previous / Next buttons
 */
export function PaginationFooter({
  total,
  page,
  pageSize,
  onPageSizeChange,
  onPageChange,
}: PaginationFooterProps) {
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  const from = total === 0 ? 0 : page * pageSize + 1;
  const to = Math.min((page + 1) * pageSize, total);

  return (
    <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between border-t border-slate-200 bg-slate-50 px-4 py-3">
      {/* Items-per-page selector */}
      <div className="flex items-center gap-2 text-xs text-slate-500">
        <label htmlFor="page-size-select" className="whitespace-nowrap">
          Rows per page:
        </label>
        <select
          id="page-size-select"
          value={pageSize}
          onChange={(e) => {
            onPageSizeChange(Number(e.target.value) as PageSizeOption);
          }}
          className="rounded-md border border-slate-300 bg-white px-2 py-1 text-xs text-slate-700 focus:outline-none focus:ring-2 focus:ring-indigo-300"
          aria-label="Rows per page"
        >
          {PAGE_SIZE_OPTIONS.map((size) => (
            <option key={size} value={size}>
              {size}
            </option>
          ))}
        </select>
      </div>

      {/* Range label + navigation */}
      <div className="flex items-center gap-3 text-xs text-slate-500">
        <span>
          {from}–{to} of {total}
        </span>
        <div className="flex gap-1">
          <button
            disabled={page === 0}
            onClick={() => onPageChange(page - 1)}
            aria-label="Previous page"
            className="rounded-lg border border-slate-300 px-3 py-1 text-sm hover:bg-white disabled:opacity-40 transition-colors"
          >
            ← Prev
          </button>
          <button
            disabled={page >= totalPages - 1}
            onClick={() => onPageChange(page + 1)}
            aria-label="Next page"
            className="rounded-lg border border-slate-300 px-3 py-1 text-sm hover:bg-white disabled:opacity-40 transition-colors"
          >
            Next →
          </button>
        </div>
      </div>
    </div>
  );
}
