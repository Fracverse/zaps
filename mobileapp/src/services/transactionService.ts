/**
 * Transaction service — fetches from backend API with local AsyncStorage cache.
 * Falls back to cache when offline.
 */
import AsyncStorage from "@react-native-async-storage/async-storage";
import {
  Transaction,
  TransactionPage,
  TransactionFilters,
} from "../types/transaction";

const CACHE_KEY = "tx_cache_v1";
const CACHE_TTL_MS = 5 * 60 * 1000; // 5 minutes
const PAGE_SIZE = 20;

// 30-second in-memory cache to deduplicate rapid identical requests
const memCache = new Map<string, { data: TransactionPage; expiresAt: number }>();

// ── Mock data (replace with real API base URL from config) ────────────────────
const MOCK_TRANSACTIONS: Transaction[] = [
  {
    id: "tx_001",
    type: "sent",
    status: "completed",
    amount: "50.00",
    asset: "USDC",
    fiatValue: "50.00",
    fiatCurrency: "USD",
    address: "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5",
    addressLabel: "alice.blink",
    timestamp: new Date(Date.now() - 1 * 60 * 60 * 1000).toISOString(),
    stellarTxHash:
      "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
    memo: "Lunch split",
    fee: "0.00001",
    feeAsset: "XLM",
    network: "Stellar",
  },
  {
    id: "tx_002",
    type: "received",
    status: "completed",
    amount: "120.00",
    asset: "USDC",
    fiatValue: "120.00",
    fiatCurrency: "USD",
    address: "GCEZWKCA5VLDNRLN3RPRJMRZOX3Z6G5CHCGZXG5CPCJDGX4LNZM4IXX",
    addressLabel: "bob.blink",
    timestamp: new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString(),
    stellarTxHash:
      "b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3",
    fee: "0.00001",
    feeAsset: "XLM",
    network: "Stellar",
  },
  {
    id: "tx_003",
    type: "sent",
    status: "pending",
    amount: "25.50",
    asset: "XLM",
    fiatValue: "3.18",
    fiatCurrency: "USD",
    address: "GDQOE23CFSUMSVQK4Y5JHPPYK73VYCNHZHA7ENKCV37P6SUEO6XQBKPP",
    timestamp: new Date(Date.now() - 6 * 60 * 60 * 1000).toISOString(),
    stellarTxHash:
      "c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4",
    fee: "0.00001",
    feeAsset: "XLM",
    network: "Stellar",
  },
  {
    id: "tx_004",
    type: "received",
    status: "completed",
    amount: "200.00",
    asset: "USDT",
    fiatValue: "200.00",
    fiatCurrency: "USD",
    address: "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H",
    addressLabel: "carol.blink",
    timestamp: new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString(),
    stellarTxHash:
      "d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5",
    memo: "Invoice #1042",
    fee: "0.00001",
    feeAsset: "XLM",
    network: "Stellar",
  },
  {
    id: "tx_005",
    type: "sent",
    status: "failed",
    amount: "10.00",
    asset: "USDC",
    fiatValue: "10.00",
    fiatCurrency: "USD",
    address: "GCFONE23CFSUMSVQK4Y5JHPPYK73VYCNHZHA7ENKCV37P6SUEO6XQBKPP",
    timestamp: new Date(Date.now() - 2 * 24 * 60 * 60 * 1000).toISOString(),
    stellarTxHash:
      "e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6",
    fee: "0.00001",
    feeAsset: "XLM",
    network: "Stellar",
  },
  {
    id: "tx_006",
    type: "received",
    status: "completed",
    amount: "75.00",
    asset: "USDC",
    fiatValue: "75.00",
    fiatCurrency: "USD",
    address: "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5",
    addressLabel: "alice.blink",
    timestamp: new Date(Date.now() - 3 * 24 * 60 * 60 * 1000).toISOString(),
    stellarTxHash:
      "f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1",
    memo: "Rent share",
    fee: "0.00001",
    feeAsset: "XLM",
    network: "Stellar",
  },
  {
    id: "tx_007",
    type: "sent",
    status: "completed",
    amount: "500.00",
    asset: "USDC",
    fiatValue: "500.00",
    fiatCurrency: "USD",
    address: "GCEZWKCA5VLDNRLN3RPRJMRZOX3Z6G5CHCGZXG5CPCJDGX4LNZM4IXX",
    addressLabel: "bob.blink",
    timestamp: new Date(Date.now() - 5 * 24 * 60 * 60 * 1000).toISOString(),
    stellarTxHash:
      "a2b3c4d5e6f7a2b3c4d5e6f7a2b3c4d5e6f7a2b3c4d5e6f7a2b3c4d5e6f7a2b3",
    memo: "Project payment",
    fee: "0.00001",
    feeAsset: "XLM",
    network: "Stellar",
  },
  {
    id: "tx_008",
    type: "received",
    status: "pending",
    amount: "30.00",
    asset: "XLM",
    fiatValue: "3.75",
    fiatCurrency: "USD",
    address: "GDQOE23CFSUMSVQK4Y5JHPPYK73VYCNHZHA7ENKCV37P6SUEO6XQBKPP",
    timestamp: new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString(),
    stellarTxHash:
      "b3c4d5e6f7a2b3c4d5e6f7a2b3c4d5e6f7a2b3c4d5e6f7a2b3c4d5e6f7a2b3c4",
    fee: "0.00001",
    feeAsset: "XLM",
    network: "Stellar",
  },
];

interface CacheEntry {
  data: Transaction[];
  timestamp: number;
}

async function readCache(): Promise<Transaction[] | null> {
  try {
    const raw = await AsyncStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const entry: CacheEntry = JSON.parse(raw);
    if (Date.now() - entry.timestamp > CACHE_TTL_MS) return null;
    return entry.data;
  } catch {
    return null;
  }
}

async function writeCache(data: Transaction[]): Promise<void> {
  try {
    const entry: CacheEntry = { data, timestamp: Date.now() };
    await AsyncStorage.setItem(CACHE_KEY, JSON.stringify(entry));
  } catch {
    // non-fatal
  }
}

/** Fetch a page of transactions, with offline cache fallback. */
export async function fetchTransactions(
  filters: TransactionFilters,
  cursor: string | null
): Promise<TransactionPage> {
  const cacheKey = JSON.stringify({ filters, cursor });
  const cached = memCache.get(cacheKey);
  if (cached && Date.now() < cached.expiresAt) return cached.data;

  // In production replace this block with a real API call:
  // const res = await fetchWithRetry(`${API_BASE}/transactions?cursor=${cursor}&...`);
  // const json = await res.json();

  // Simulate network latency
  await new Promise((r) => setTimeout(r, 600));

  let all = MOCK_TRANSACTIONS;

  // Try to merge with cache (real app would just use API response)
  const cached = await readCache();
  if (cached) {
    const cachedIds = new Set(cached.map((t) => t.id));
    const fresh = all.filter((t) => !cachedIds.has(t.id));
    all = [...fresh, ...cached];
  }

  // Apply filters
  let filtered = all.filter((tx) => {
    if (filters.type !== "all" && tx.type !== filters.type) return false;
    if (filters.status !== "all" && tx.status !== filters.status) return false;
    if (filters.search) {
      const q = filters.search.toLowerCase();
      const matchAddr = tx.address.toLowerCase().includes(q);
      const matchLabel = tx.addressLabel?.toLowerCase().includes(q) ?? false;
      const matchMemo = tx.memo?.toLowerCase().includes(q) ?? false;
      const matchHash = tx.stellarTxHash?.toLowerCase().includes(q) ?? false;
      const matchAsset = tx.asset.toLowerCase().includes(q);
      if (!matchAddr && !matchLabel && !matchMemo && !matchHash && !matchAsset)
        return false;
    }
    if (filters.dateFrom) {
      if (new Date(tx.timestamp) < new Date(filters.dateFrom)) return false;
    }
    if (filters.dateTo) {
      if (new Date(tx.timestamp) > new Date(filters.dateTo)) return false;
    }
    if (filters.amountMin) {
      if (parseFloat(tx.amount) < parseFloat(filters.amountMin)) return false;
    }
    if (filters.amountMax) {
      if (parseFloat(tx.amount) > parseFloat(filters.amountMax)) return false;
    }
    return true;
  });

  // Sort newest first
  filtered.sort(
    (a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
  );

  // Cursor-based pagination
  const startIndex = cursor
    ? filtered.findIndex((t) => t.id === cursor) + 1
    : 0;
  const page = filtered.slice(startIndex, startIndex + PAGE_SIZE);
  const nextCursor =
    startIndex + PAGE_SIZE < filtered.length
      ? (page[page.length - 1]?.id ?? null)
      : null;

  // Update cache with latest full list
  await writeCache(all);

  const result: TransactionPage = { items: page, nextCursor, total: filtered.length };
  memCache.set(cacheKey, { data: result, expiresAt: Date.now() + 30_000 });
  return result;
}

/** Fetch a single transaction by id (cache-first). */
export async function fetchTransactionById(
  id: string
): Promise<Transaction | null> {
  const cached = await readCache();
  if (cached) {
    const found = cached.find((t) => t.id === id);
    if (found) return found;
  }
  return MOCK_TRANSACTIONS.find((t) => t.id === id) ?? null;
}

/** Invalidate the local cache (call after a new transaction is submitted). */
export async function invalidateTransactionCache(): Promise<void> {
  memCache.clear();
  await AsyncStorage.removeItem(CACHE_KEY);
}
