import { useState, useCallback, useRef, useEffect } from "react";
import { fetchTransactions } from "../services/transactionService";
import { Transaction, TransactionFilters } from "../types/transaction";

const DEFAULT_FILTERS: TransactionFilters = {
  type: "all",
  status: "all",
  search: "",
};

export function useTransactions() {
  const [items, setItems] = useState<Transaction[]>([]);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [offlineCache, setOfflineCache] = useState(false);
  const [filters, setFiltersState] =
    useState<TransactionFilters>(DEFAULT_FILTERS);
  const [total, setTotal] = useState(0);

  const cursorRef = useRef<string | null>(null);
  const hasMoreRef = useRef(true);
  const activeFiltersRef = useRef(filters);

  const load = useCallback(
    async (
      newFilters: TransactionFilters,
      cursor: string | null,
      append: boolean
    ) => {
      try {
        const page = await fetchTransactions(newFilters, cursor);
        setOfflineCache(page.source === "cache");
        cursorRef.current = page.nextCursor;
        hasMoreRef.current = page.nextCursor !== null;
        setTotal(page.total);
        setItems((prev) => {
          if (!append) return page.items;
          // Guard against duplicates when the backend returns overlapping items
          // across cursor pages (e.g. a new tx arriving mid-scroll).
          const seen = new Set(prev.map((t) => t.id));
          return [...prev, ...page.items.filter((t) => !seen.has(t.id))];
        });
        setError(null);
      } catch (e: unknown) {
        const msg =
          e instanceof Error ? e.message : "Failed to load transactions";
        setError(msg);
      }
    },
    []
  );

  /** Initial load / filter change */
  const reload = useCallback(
    async (newFilters?: TransactionFilters) => {
      const f = newFilters ?? activeFiltersRef.current;
      activeFiltersRef.current = f;
      cursorRef.current = null;
      hasMoreRef.current = true;
      setLoading(true);
      await load(f, null, false);
      setLoading(false);
    },
    [load]
  );

  /** Pull-to-refresh */
  const refresh = useCallback(async () => {
    setRefreshing(true);
    cursorRef.current = null;
    hasMoreRef.current = true;
    await load(activeFiltersRef.current, null, false);
    setRefreshing(false);
  }, [load]);

  /** Infinite scroll — load next page */
  const loadMore = useCallback(async () => {
    if (loadingMore || !hasMoreRef.current || cursorRef.current === null)
      return;
    setLoadingMore(true);
    await load(activeFiltersRef.current, cursorRef.current, true);
    setLoadingMore(false);
  }, [load, loadingMore]);

  /** Update filters and reload */
  const applyFilters = useCallback(
    (newFilters: TransactionFilters) => {
      setFiltersState(newFilters);
      reload(newFilters);
    },
    [reload]
  );

  const setFilters = useCallback(
    (
      update:
        TransactionFilters | ((prev: TransactionFilters) => TransactionFilters)
    ) => {
      setFiltersState((prev) => {
        const next = typeof update === "function" ? update(prev) : update;
        activeFiltersRef.current = next;
        cursorRef.current = null;
        hasMoreRef.current = true;
        void reload(next);
        return next;
      });
    },
    [reload]
  );

  useEffect(() => {
    void reload();
  }, [reload]);

  return {
    items,
    loading,
    refreshing,
    loadingMore,
    error,
    offlineCache,
    filters,
    total,
    hasMore: hasMoreRef.current,
    reload,
    refresh,
    loadMore,
    applyFilters,
    setFilters,
  };
}
