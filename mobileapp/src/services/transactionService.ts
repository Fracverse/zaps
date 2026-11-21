import AsyncStorage from "@react-native-async-storage/async-storage";
import { apiFetch, getSessionToken } from "./api";
import type {
  Transaction,
  TransactionPage,
  TransactionFilters,
} from "../types/transaction";

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";
const PAGE_SIZE = 20;
const CACHE_PREFIX = "tx_server_page_v2:";
const CACHE_TTL_MS = 30_000;

const memCache = new Map<
  string,
  { data: TransactionPage; expiresAt: number }
>();

interface ApiTransaction {
  id: string;
  type: Transaction["type"];
  status: string;
  amount: string | number;
  asset: string;
  fiat_value?: string | number;
  fiat_currency?: string;
  address: string;
  address_label?: string | null;
  timestamp?: string;
  created_at?: string;
  stellar_tx_hash?: string | null;
  tx_hash?: string | null;
  memo?: string | null;
  yield_type?: Transaction["yieldType"] | null;
  sender_name?: string | null;
  sender_anchor?: string | null;
}

interface ApiPage {
  items: ApiTransaction[];
  next_cursor: string | null;
  total: number;
}

interface CachedPage {
  page: TransactionPage;
  savedAt: number;
}

class HttpError extends Error {
  constructor(readonly status: number) {
    super(`HTTP ${status}`);
  }
}

function sessionKey(token: string | null): string {
  let hash = 2166136261;
  for (const character of token ?? "anonymous") {
    hash = Math.imul(hash ^ character.charCodeAt(0), 16777619);
  }
  return (hash >>> 0).toString(36);
}

function toTransaction(raw: ApiTransaction): Transaction {
  const amount = String(raw.amount);
  return {
    id: raw.id,
    type: raw.type,
    status: raw.status.toLowerCase() as Transaction["status"],
    amount,
    asset: raw.asset,
    fiatValue: String(raw.fiat_value ?? amount),
    fiatCurrency:
      raw.fiat_currency ?? (raw.asset === "USDC" ? "USD" : raw.asset),
    address: raw.address,
    addressLabel: raw.address_label ?? undefined,
    timestamp: raw.timestamp ?? raw.created_at ?? "",
    stellarTxHash: raw.stellar_tx_hash ?? raw.tx_hash ?? undefined,
    memo: raw.memo ?? undefined,
    yieldType: raw.yield_type ?? undefined,
    senderName: raw.sender_name ?? undefined,
    senderAnchor: raw.sender_anchor ?? undefined,
  };
}

function makeCacheKey(
  filters: TransactionFilters,
  cursor: string | null,
  user: string
) {
  return `${CACHE_PREFIX}${user}:${JSON.stringify({ filters, cursor })}`;
}

async function readCachedPage(key: string): Promise<TransactionPage | null> {
  try {
    const raw = await AsyncStorage.getItem(key);
    if (!raw) return null;
    const cached = JSON.parse(raw) as CachedPage;
    return cached?.page ?? null;
  } catch {
    return null;
  }
}

async function storeServerPage(
  key: string,
  page: TransactionPage
): Promise<void> {
  try {
    await AsyncStorage.setItem(
      key,
      JSON.stringify({ page, savedAt: Date.now() } satisfies CachedPage)
    );
  } catch {
    // A cache write must not make a successful server response fail.
  }
}

/** Fetch one authenticated history page; only successful server responses are cached. */
export async function fetchTransactions(
  filters: TransactionFilters,
  cursor: string | null
): Promise<TransactionPage> {
  const token = await getSessionToken();
  const key = makeCacheKey(filters, cursor, sessionKey(token));
  const cachedInMemory = memCache.get(key);
  if (cachedInMemory && Date.now() < cachedInMemory.expiresAt) {
    return cachedInMemory.data;
  }

  const params = new URLSearchParams({ limit: String(PAGE_SIZE) });
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

  try {
    const response = await apiFetch(
      `${API_BASE}/api/transactions/history?${params.toString()}`
    );
    if (!response.ok) throw new HttpError(response.status);
    const payload = (await response.json()) as ApiPage;
    const page: TransactionPage = {
      items: Array.isArray(payload.items)
        ? payload.items.map(toTransaction)
        : [],
      nextCursor: payload.next_cursor ?? null,
      total: Number(payload.total ?? 0),
      offlineCache: false,
    };
    if ((await getSessionToken()) === token) {
      await storeServerPage(key, page);
      memCache.set(key, { data: page, expiresAt: Date.now() + CACHE_TTL_MS });
    }
    return page;
  } catch (error) {
    if (error instanceof HttpError) throw error;
    if (!(error instanceof TypeError)) throw error;
    const cachedPage = await readCachedPage(key);
    if (cachedPage) return { ...cachedPage, offlineCache: true };
    throw error;
  }
}

/** Fetch an authenticated transaction detail. Unknown IDs are a normal miss. */
export async function fetchTransactionById(
  id: string
): Promise<Transaction | null> {
  const response = await apiFetch(
    `${API_BASE}/api/transactions/history/${encodeURIComponent(id)}`
  );
  if (response.status === 404 || response.status === 400) return null;
  if (!response.ok) throw new HttpError(response.status);
  return toTransaction((await response.json()) as ApiTransaction);
}

export async function invalidateTransactionCache(): Promise<void> {
  memCache.clear();
  try {
    const keys = await AsyncStorage.getAllKeys();
    const transactionKeys = keys.filter((key) => key.startsWith(CACHE_PREFIX));
    if (transactionKeys.length) await AsyncStorage.multiRemove(transactionKeys);
  } catch {
    // Ignore cache invalidation failures.
  }
}
