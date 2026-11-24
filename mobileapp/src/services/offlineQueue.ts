/**
 * offlineQueue.ts
 *
 * Offline Transaction Queue & Auto-Sync Engine (#687)
 *
 * When the device loses connectivity, outgoing Zap transaction requests are
 * persisted to AsyncStorage under `@zaps_offline_tx_queue`. A NetInfo listener
 * watches for reconnection and automatically replays the queue in FIFO order
 * with exponential back-off so that a flaky reconnect doesn't hammer the API.
 *
 * Usage:
 *   import { enqueueTransaction, startOfflineQueueSync } from './offlineQueue';
 *
 *   // Start the listener once (e.g. in the root layout):
 *   const cleanup = startOfflineQueueSync();
 *   // …later (e.g. on unmount or logout):
 *   cleanup();
 *
 *   // Queue a failed / offline transaction:
 *   await enqueueTransaction({ endpoint, method, body });
 */

import AsyncStorage from "@react-native-async-storage/async-storage";
import NetInfo from "@react-native-community/netinfo";

// ── Storage key ───────────────────────────────────────────────────────────────

export const OFFLINE_QUEUE_KEY = "@zaps_offline_tx_queue";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface QueuedTransaction {
  /** Unique identifier for idempotency tracking. */
  id: string;
  /** Full API URL including base (e.g. https://api.zaps.app/api/v1/transfer). */
  endpoint: string;
  /** HTTP method (POST, PUT, PATCH, …). */
  method: string;
  /** Request body to replay. */
  body: Record<string, unknown>;
  /** Authorization token captured at queue time so the replay carries auth. */
  authToken?: string;
  /** ISO-8601 timestamp when the item was first enqueued. */
  enqueuedAt: string;
  /** Number of times the item has been retried. */
  retryCount: number;
  /** Unix-ms timestamp after which the next retry is allowed (exponential back-off). */
  nextRetryAt: number;
}

export interface EnqueueOptions {
  endpoint: string;
  method?: string;
  body: Record<string, unknown>;
  /** Pass the current session token so the replay can authenticate. */
  authToken?: string;
}

// ── Constants ─────────────────────────────────────────────────────────────────

/** Maximum number of retry attempts before a transaction is dropped. */
export const MAX_RETRIES = 5;

/** Base delay (ms) for exponential back-off: delay = BASE * 2^retryCount. */
const BACKOFF_BASE_MS = 2_000;

/** Upper bound for back-off delay (30 s). */
const BACKOFF_MAX_MS = 30_000;

// ── Internal state ────────────────────────────────────────────────────────────

/** Prevents concurrent queue-flush runs. */
let _isFlushing = false;

/** Tracks whether the previous NetInfo state was connected. */
let _wasConnected: boolean | null = null;

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Generate a simple pseudo-random ID. */
function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

/** Calculate the next retry delay with exponential back-off and jitter. */
function calcBackoffMs(retryCount: number): number {
  const exp = Math.min(BACKOFF_BASE_MS * Math.pow(2, retryCount), BACKOFF_MAX_MS);
  // Add up to 20% jitter so multiple queued items don't all fire at once.
  const jitter = exp * 0.2 * Math.random();
  return Math.floor(exp + jitter);
}

// ── AsyncStorage helpers ──────────────────────────────────────────────────────

/**
 * Read the full persisted queue from AsyncStorage.
 * Returns an empty array if there is nothing stored or parsing fails.
 */
export async function readQueue(): Promise<QueuedTransaction[]> {
  try {
    const raw = await AsyncStorage.getItem(OFFLINE_QUEUE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed as QueuedTransaction[];
  } catch {
    return [];
  }
}

/**
 * Persist the queue back to AsyncStorage.
 */
export async function writeQueue(queue: QueuedTransaction[]): Promise<void> {
  try {
    await AsyncStorage.setItem(OFFLINE_QUEUE_KEY, JSON.stringify(queue));
  } catch {
    // Non-fatal: best-effort persistence.
  }
}

/**
 * Append a new transaction payload to the tail of the persisted queue.
 */
export async function enqueueTransaction(opts: EnqueueOptions): Promise<void> {
  const queue = await readQueue();
  const item: QueuedTransaction = {
    id: generateId(),
    endpoint: opts.endpoint,
    method: opts.method ?? "POST",
    body: opts.body,
    authToken: opts.authToken,
    enqueuedAt: new Date().toISOString(),
    retryCount: 0,
    nextRetryAt: Date.now(), // eligible for immediate replay on reconnect
  };
  queue.push(item);
  await writeQueue(queue);
}

/**
 * Remove a successfully transmitted transaction from the queue by ID.
 */
export async function removeFromQueue(id: string): Promise<void> {
  const queue = await readQueue();
  const filtered = queue.filter((item) => item.id !== id);
  await writeQueue(filtered);
}

/**
 * Return the current number of items waiting in the queue.
 */
export async function getQueueLength(): Promise<number> {
  const queue = await readQueue();
  return queue.length;
}

// ── Network fetch with retry logic ────────────────────────────────────────────

/**
 * Attempt to transmit a single queued transaction.
 * Returns `true` on success (HTTP 2xx), `false` on network or server failure.
 */
async function transmit(item: QueuedTransaction): Promise<boolean> {
  try {
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      ...(item.authToken ? { Authorization: `Bearer ${item.authToken}` } : {}),
    };

    const response = await fetch(item.endpoint, {
      method: item.method,
      headers,
      body: JSON.stringify(item.body),
    });

    // 2xx → success; 4xx client errors are also considered "done" (won't
    // succeed on retry) — remove them rather than re-queuing indefinitely.
    if (response.ok || (response.status >= 400 && response.status < 500)) {
      return true;
    }

    // 5xx → server error; retry later.
    return false;
  } catch {
    // Network error (no connection, DNS failure, etc.) → retry later.
    return false;
  }
}

// ── Queue flush (sync engine) ─────────────────────────────────────────────────

/**
 * Process the offline queue sequentially, oldest item first.
 *
 * Each item is only attempted if its `nextRetryAt` timestamp has passed.
 * On failure the item's `retryCount` and `nextRetryAt` are updated for the
 * next reconnect cycle. Items that exceed `MAX_RETRIES` are dropped.
 */
export async function flushQueue(): Promise<void> {
  if (_isFlushing) return;
  _isFlushing = true;

  try {
    const queue = await readQueue();
    if (queue.length === 0) return;

    const now = Date.now();
    const updated: QueuedTransaction[] = [];

    for (const item of queue) {
      // Skip if still in back-off window.
      if (item.nextRetryAt > now) {
        updated.push(item);
        continue;
      }

      const success = await transmit(item);

      if (success) {
        // Successfully sent — discard from queue.
        continue;
      }

      const newRetryCount = item.retryCount + 1;

      if (newRetryCount >= MAX_RETRIES) {
        // Exhausted retries — drop the item to avoid queue bloat.
        console.warn(
          `[offlineQueue] Dropping transaction ${item.id} after ${MAX_RETRIES} failed attempts.`
        );
        continue;
      }

      // Schedule next retry with exponential back-off.
      updated.push({
        ...item,
        retryCount: newRetryCount,
        nextRetryAt: now + calcBackoffMs(newRetryCount),
      });
    }

    await writeQueue(updated);
  } finally {
    _isFlushing = false;
  }
}

// ── NetInfo listener ──────────────────────────────────────────────────────────

/**
 * Subscribe to network state changes and trigger `flushQueue()` whenever
 * the device goes from offline → online.
 *
 * @returns A cleanup function that removes the NetInfo listener.
 *
 * @example
 * // In your root layout (_layout.tsx):
 * useEffect(() => {
 *   const cleanup = startOfflineQueueSync();
 *   return cleanup;
 * }, []);
 */
export function startOfflineQueueSync(): () => void {
  const unsubscribe = NetInfo.addEventListener((state) => {
    const isConnected = state.isConnected === true;

    // Trigger flush only on the offline → online transition.
    if (isConnected && _wasConnected === false) {
      flushQueue().catch((err) => {
        console.warn("[offlineQueue] Flush error:", err);
      });
    }

    _wasConnected = isConnected;
  });

  // Also do an immediate check in case the app launches while online.
  NetInfo.fetch().then((state) => {
    _wasConnected = state.isConnected === true;
    if (_wasConnected) {
      flushQueue().catch((err) => {
        console.warn("[offlineQueue] Initial flush error:", err);
      });
    }
  });

  return unsubscribe;
}
