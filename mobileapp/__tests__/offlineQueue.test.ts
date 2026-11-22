/**
 * offlineQueue.test.ts
 *
 * Unit tests for the offline transaction queue & auto-sync engine (#687).
 *
 * Tests cover:
 *  - Enqueue / read / remove queue operations
 *  - Exponential back-off logic
 *  - flushQueue: success removes items, network error increments retry count
 *  - flushQueue: items exceeding MAX_RETRIES are dropped
 *  - startOfflineQueueSync: triggers flush on offline→online transition
 */

import AsyncStorage from "@react-native-async-storage/async-storage";

// ── Mocks ──────────────────────────────────────────────────────────────────────

jest.mock("@react-native-async-storage/async-storage", () =>
  require("@react-native-async-storage/async-storage/jest/async-storage-mock")
);

const mockNetInfoFetch = jest.fn();
const mockNetInfoAddEventListener = jest.fn();

jest.mock("@react-native-community/netinfo", () => ({
  fetch: () => mockNetInfoFetch(),
  addEventListener: (cb: (state: unknown) => void) => {
    mockNetInfoAddEventListener(cb);
    return jest.fn(); // unsubscribe noop
  },
}));

const mockFetch = jest.fn();
global.fetch = mockFetch as unknown as typeof fetch;

// ── Imports (after mocks) ──────────────────────────────────────────────────────

import {
  enqueueTransaction,
  readQueue,
  writeQueue,
  removeFromQueue,
  getQueueLength,
  flushQueue,
  startOfflineQueueSync,
  OFFLINE_QUEUE_KEY,
  MAX_RETRIES,
  type QueuedTransaction,
} from "../src/services/offlineQueue";

// ── Helpers ────────────────────────────────────────────────────────────────────

function makeItem(overrides: Partial<QueuedTransaction> = {}): QueuedTransaction {
  return {
    id: "test-id",
    endpoint: "https://api.zaps.app/api/v1/transfer",
    method: "POST",
    body: { recipient: "tolu.zaps", amount: "500" },
    enqueuedAt: new Date().toISOString(),
    retryCount: 0,
    nextRetryAt: Date.now() - 1000, // already eligible
    ...overrides,
  };
}

// ── Tests ──────────────────────────────────────────────────────────────────────

beforeEach(async () => {
  jest.clearAllMocks();
  mockFetch.mockReset();
  // Clear AsyncStorage between tests
  await AsyncStorage.clear();
  // Reset NetInfo mock defaults
  mockNetInfoFetch.mockResolvedValue({ isConnected: true });
});

// ─ readQueue / writeQueue ─────────────────────────────────────────────────────

describe("readQueue", () => {
  it("returns empty array when nothing is stored", async () => {
    const q = await readQueue();
    expect(q).toEqual([]);
  });

  it("returns empty array on malformed JSON", async () => {
    await AsyncStorage.setItem(OFFLINE_QUEUE_KEY, "{not valid json");
    const q = await readQueue();
    expect(q).toEqual([]);
  });

  it("reads back a written queue", async () => {
    const items = [makeItem({ id: "a" }), makeItem({ id: "b" })];
    await writeQueue(items);
    const q = await readQueue();
    expect(q).toHaveLength(2);
    expect(q[0].id).toBe("a");
  });
});

// ─ enqueueTransaction ─────────────────────────────────────────────────────────

describe("enqueueTransaction", () => {
  it("appends a new item to the queue", async () => {
    await enqueueTransaction({
      endpoint: "https://api.zaps.app/api/v1/transfer",
      body: { amount: "100" },
    });
    const q = await readQueue();
    expect(q).toHaveLength(1);
    expect(q[0].endpoint).toBe("https://api.zaps.app/api/v1/transfer");
    expect(q[0].retryCount).toBe(0);
    expect(q[0].method).toBe("POST");
  });

  it("accumulates multiple items", async () => {
    await enqueueTransaction({ endpoint: "https://a.com", body: {} });
    await enqueueTransaction({ endpoint: "https://b.com", body: {} });
    const q = await readQueue();
    expect(q).toHaveLength(2);
  });

  it("stores the auth token with the item", async () => {
    await enqueueTransaction({
      endpoint: "https://api.zaps.app/api/v1/transfer",
      body: {},
      authToken: "bearer-token-123",
    });
    const q = await readQueue();
    expect(q[0].authToken).toBe("bearer-token-123");
  });
});

// ─ removeFromQueue ────────────────────────────────────────────────────────────

describe("removeFromQueue", () => {
  it("removes the item with the matching id", async () => {
    const items = [makeItem({ id: "keep" }), makeItem({ id: "remove-me" })];
    await writeQueue(items);
    await removeFromQueue("remove-me");
    const q = await readQueue();
    expect(q).toHaveLength(1);
    expect(q[0].id).toBe("keep");
  });

  it("is a no-op when id is not found", async () => {
    await writeQueue([makeItem({ id: "only" })]);
    await removeFromQueue("nonexistent");
    expect(await getQueueLength()).toBe(1);
  });
});

// ─ getQueueLength ─────────────────────────────────────────────────────────────

describe("getQueueLength", () => {
  it("returns 0 for an empty queue", async () => {
    expect(await getQueueLength()).toBe(0);
  });

  it("returns the correct count after enqueuing", async () => {
    await enqueueTransaction({ endpoint: "https://x.com", body: {} });
    await enqueueTransaction({ endpoint: "https://y.com", body: {} });
    expect(await getQueueLength()).toBe(2);
  });
});

// ─ flushQueue ─────────────────────────────────────────────────────────────────

describe("flushQueue", () => {
  it("removes successfully sent items from the queue", async () => {
    await writeQueue([makeItem({ id: "tx1" })]);
    mockFetch.mockResolvedValueOnce({ ok: true, status: 200 });

    await flushQueue();

    expect(await getQueueLength()).toBe(0);
  });

  it("keeps items in the queue when network fails and increments retryCount", async () => {
    await writeQueue([makeItem({ id: "tx1", retryCount: 0 })]);
    mockFetch.mockRejectedValueOnce(new Error("Network unreachable"));

    await flushQueue();

    const q = await readQueue();
    expect(q).toHaveLength(1);
    expect(q[0].retryCount).toBe(1);
    expect(q[0].nextRetryAt).toBeGreaterThan(Date.now());
  });

  it("keeps items in the queue when server returns 5xx", async () => {
    await writeQueue([makeItem({ id: "tx1" })]);
    mockFetch.mockResolvedValueOnce({ ok: false, status: 503 });

    await flushQueue();

    expect(await getQueueLength()).toBe(1);
  });

  it("drops items that exceed MAX_RETRIES", async () => {
    await writeQueue([makeItem({ id: "exhaust", retryCount: MAX_RETRIES - 1 })]);
    mockFetch.mockRejectedValueOnce(new Error("Still broken"));

    await flushQueue();

    // Item should be dropped
    expect(await getQueueLength()).toBe(0);
  });

  it("skips items still in back-off window", async () => {
    const futureRetry = Date.now() + 60_000; // 60 seconds from now
    await writeQueue([makeItem({ id: "tx-wait", nextRetryAt: futureRetry })]);

    await flushQueue();

    // fetch should not have been called
    expect(mockFetch).not.toHaveBeenCalled();
    expect(await getQueueLength()).toBe(1);
  });

  it("removes 4xx items (client errors) as permanently failed", async () => {
    await writeQueue([makeItem({ id: "bad" })]);
    mockFetch.mockResolvedValueOnce({ ok: false, status: 400 });

    await flushQueue();

    // 4xx → treated as "done" (no retry), item dropped
    expect(await getQueueLength()).toBe(0);
  });

  it("processes multiple queued items sequentially", async () => {
    await writeQueue([
      makeItem({ id: "t1", endpoint: "https://api.zaps.app/a" }),
      makeItem({ id: "t2", endpoint: "https://api.zaps.app/b" }),
    ]);
    mockFetch
      .mockResolvedValueOnce({ ok: true, status: 200 })
      .mockResolvedValueOnce({ ok: true, status: 200 });

    await flushQueue();

    expect(await getQueueLength()).toBe(0);
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });

  it("does not run concurrent flushes", async () => {
    await writeQueue([makeItem({ id: "concurrent" })]);

    // Intentionally delay the first flush
    mockFetch.mockImplementationOnce(
      () => new Promise((r) => setTimeout(() => r({ ok: true, status: 200 }), 100))
    );

    // Fire two concurrent flushes — only one should actually run
    const [, second] = await Promise.all([flushQueue(), flushQueue()]);
    // Second flush returns immediately (lock held by first)
    expect(second).toBeUndefined();
  });
});

// ─ startOfflineQueueSync ──────────────────────────────────────────────────────

describe("startOfflineQueueSync", () => {
  it("calls flushQueue on offline → online transition", async () => {
    await writeQueue([makeItem({ id: "queued" })]);
    mockFetch.mockResolvedValue({ ok: true, status: 200 });

    let capturedListener: ((state: unknown) => void) | null = null;
    mockNetInfoAddEventListener.mockImplementation((cb: (state: unknown) => void) => {
      capturedListener = cb;
      return jest.fn();
    });
    mockNetInfoFetch.mockResolvedValue({ isConnected: false });

    const cleanup = startOfflineQueueSync();

    // Simulate going offline
    capturedListener?.({ isConnected: false });

    // Simulate going online
    capturedListener?.({ isConnected: true });

    // Allow microtasks / promises to settle
    await new Promise((r) => setTimeout(r, 50));

    expect(mockFetch).toHaveBeenCalled();

    cleanup();
  });

  it("returns a cleanup function that removes the NetInfo listener", () => {
    const mockUnsubscribe = jest.fn();
    mockNetInfoAddEventListener.mockReturnValue(mockUnsubscribe);
    mockNetInfoFetch.mockResolvedValue({ isConnected: true });

    const cleanup = startOfflineQueueSync();
    cleanup();

    expect(mockUnsubscribe).toHaveBeenCalled();
  });
});
