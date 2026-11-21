/**
 * Authenticated transaction history and detail service.
 * Only successful backend responses are cached.
 */
import AsyncStorage from "@react-native-async-storage/async-storage";
import { apiFetch } from "./api";
import {
  Transaction,
  TransactionPage,
  TransactionFilters,
} from "../types/transaction";

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";
const CACHE_KEY = "tx_cache_v1";
const CACHE_TTL_MS = 5 * 60 * 1000;

interface CacheEntry {
  data: Transaction[];
  timestamp: number;
}

const memCache = new Map<
  string,
  { data: TransactionPage; expiresAt: number }
>();

async function readCache(): Promise<Transaction[] | null> {
  try {
    const raw = await AsyncStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const entry: CacheEntry = JSON.parse(raw);
    if (Date.now() - entry.timestamp > CACHE_TTL_MS) return null;
    return Array.isArray(entry.data) ? entry.data : null;
  } catch {
    return null;
  }
}

async function writeCache(data: Transaction[]): Promise<void> {
  try {
    await AsyncStorage.setItem(
      CACHE_KEY,
      JSON.stringify({ data, timestamp: Date.now() })
    );
  } catch {
    // Cache writes are best-effort; the server remains authoritative.
  }
}

function buildParams(
  filters: TransactionFilters,
  cursor: string | null
): URLSearchParams {
  const params = new URLSearchParams();
  if (cursor) params.set("cursor", cursor);
  const effectiveType =
    (filters.yieldOnly || filters.yieldType) && filters.type === "all"
      ? "yield"
      : filters.type;
  if (effectiveType !== "all") params.set("type", effectiveType);
  if (filters.yieldType) params.set("yield_type", filters.yieldType);
  if (filters.status !== "all") params.set("status", filters.status);
  if (filters.asset) params.set("asset", filters.asset);
  if (filters.dateFrom) params.set("date_from", filters.dateFrom);
  if (filters.dateTo) params.set("date_to", filters.dateTo);
  if (filters.amountMin) params.set("amount_min", filters.amountMin);
  if (filters.amountMax) params.set("amount_max", filters.amountMax);
  if (filters.search) params.set("search", filters.search);
  return params;
}

function normalizeTransaction(raw: Record<string, unknown>): Transaction {
  const type = String(raw.type ?? "payment");
  const status = String(raw.status ?? "completed");
  return {
    id: String(raw.id),
    type:
      type === "sent" ||
      type === "received" ||
      type === "swap" ||
      type === "payment" ||
      type === "yield" ||
      type === "mass_payout"
        ? (type as Transaction["type"])
        : "payment",
    status: status === "pending" || status === "failed" ? status : "completed",
    amount: String(raw.amount ?? "0"),
    asset: String(raw.asset ?? raw.currency ?? "XLM"),
    fiatValue: String(raw.fiat_value ?? raw.fiatValue ?? raw.amount ?? "0"),
    fiatCurrency: String(raw.fiat_currency ?? raw.fiatCurrency ?? "USD"),
    address: String(raw.address ?? ""),
    addressLabel:
      typeof (raw.address_label ?? raw.addressLabel) === "string"
        ? String(raw.address_label ?? raw.addressLabel)
        : undefined,
    timestamp: String(
      raw.timestamp ?? raw.created_at ?? new Date().toISOString()
    ),
    stellarTxHash:
      typeof (raw.stellar_tx_hash ?? raw.stellarTxHash) === "string"
        ? String(raw.stellar_tx_hash ?? raw.stellarTxHash)
        : undefined,
    memo: typeof raw.memo === "string" ? raw.memo : undefined,
    fee: typeof raw.fee === "string" ? raw.fee : undefined,
    feeAsset:
      typeof (raw.fee_asset ?? raw.feeAsset) === "string"
        ? String(raw.fee_asset ?? raw.feeAsset)
        : undefined,
    network: typeof raw.network === "string" ? raw.network : "Stellar",
    yieldType:
      raw.yield_type === "interest" ||
      raw.yieldType === "interest" ||
      raw.yield_type === "reward" ||
      raw.yieldType === "reward" ||
      raw.yield_type === "apy" ||
      raw.yieldType === "apy"
        ? (String(raw.yield_type ?? raw.yieldType) as Transaction["yieldType"])
        : undefined,
    stellarTxXdr:
      typeof (raw.stellar_tx_xdr ?? raw.stellarTxXdr) === "string"
        ? String(raw.stellar_tx_xdr ?? raw.stellarTxXdr)
        : undefined,
    senderName:
      typeof (raw.sender_name ?? raw.senderName) === "string"
        ? String(raw.sender_name ?? raw.senderName)
        : undefined,
    senderAnchor:
      typeof (raw.sender_anchor ?? raw.senderAnchor) === "string"
        ? String(raw.sender_anchor ?? raw.senderAnchor)
        : undefined,
  };
}

function isNetworkError(error: unknown): boolean {
  return (
    error instanceof TypeError ||
    (error instanceof Error && /network|fetch|offline/i.test(error.message))
  );
}

/** Fetch a page from the authenticated backend. */
export async function fetchTransactions(
  filters: TransactionFilters,
  cursor: string | null
): Promise<TransactionPage> {
  const cacheKey = JSON.stringify({ filters, cursor });
  const cachedResponse = memCache.get(cacheKey);
  if (cachedResponse && Date.now() < cachedResponse.expiresAt) {
    return cachedResponse.data;
  }

  const params = buildParams(filters, cursor);
  try {
    const res = await apiFetch(
      `${API_BASE}/api/transactions/${params.toString() ? `?${params}` : ""}`
    );
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error(
        typeof body.error === "string"
          ? body.error
          : `Failed to load transactions (HTTP ${res.status})`
      );
    }

    const json = (await res.json()) as Record<string, unknown>;
    const itemsRaw = Array.isArray(json.items) ? json.items : [];
    const items = itemsRaw
      .filter(
        (item): item is Record<string, unknown> =>
          typeof item === "object" && item !== null
      )
      .map(normalizeTransaction);
    const result: TransactionPage = {
      items,
      nextCursor:
        typeof json.next_cursor === "string"
          ? json.next_cursor
          : typeof json.nextCursor === "string"
            ? json.nextCursor
            : null,
      total: typeof json.total === "number" ? json.total : items.length,
      source: "server",
    };

    if (!cursor) {
      await writeCache(items);
    }
    memCache.set(cacheKey, { data: result, expiresAt: Date.now() + 30_000 });
    return result;
  } catch (error) {
    if (isNetworkError(error)) {
      const offline = await readCache();
      if (offline) {
        const result: TransactionPage = {
          items: offline,
          nextCursor: null,
          total: offline.length,
          source: "cache",
        };
        return result;
      }
    }
    throw error;
  }
}

/** Fetch one actual server transaction, with cache fallback only while offline. */
export async function fetchTransactionById(
  id: string
): Promise<Transaction | null> {
  try {
    const res = await apiFetch(
      `${API_BASE}/api/transactions/${encodeURIComponent(id)}`
    );
    if (res.status === 404) return null;
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error(
        typeof body.error === "string"
          ? body.error
          : `Failed to load transaction (HTTP ${res.status})`
      );
    }
    const json = (await res.json()) as Record<string, unknown>;
    const transaction =
      typeof json.transaction === "object" && json.transaction !== null
        ? normalizeTransaction(json.transaction as Record<string, unknown>)
        : normalizeTransaction(json);
    const cache = (await readCache()) ?? [];
    await writeCache([
      transaction,
      ...cache.filter((item) => item.id !== transaction.id),
    ]);
    return transaction;
  } catch (error) {
    if (isNetworkError(error)) {
      const cache = await readCache();
      return cache?.find((item) => item.id === id) ?? null;
    }
    throw error;
  }
}

export async function invalidateTransactionCache(): Promise<void> {
  memCache.clear();
  await AsyncStorage.removeItem(CACHE_KEY);
}
