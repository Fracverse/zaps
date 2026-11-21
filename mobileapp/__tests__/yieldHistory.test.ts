import AsyncStorage from "@react-native-async-storage/async-storage";
import { apiFetch } from "../src/services/api";
import {
  fetchTransactionById,
  fetchTransactions,
  invalidateTransactionCache,
} from "../src/services/transactionService";
import type { TransactionFilters } from "../src/types/transaction";

jest.mock("@react-native-async-storage/async-storage", () => {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  return require("@react-native-async-storage/async-storage/jest/async-storage-mock");
});
jest.mock("../src/services/api", () => ({
  apiFetch: jest.fn(),
  getSessionToken: jest.fn().mockResolvedValue("test-session"),
}));

const apiFetchMock = apiFetch as jest.Mock;
const baseFilters: TransactionFilters = {
  type: "all",
  status: "all",
  search: "",
};

function response(payload: object) {
  return { ok: true, status: 200, json: async () => payload };
}

describe("authenticated transaction history", () => {
  beforeEach(async () => {
    jest.clearAllMocks();
    jest.restoreAllMocks();
    await AsyncStorage.clear();
    await invalidateTransactionCache();
  });

  it("uses the authenticated server page and preserves filters and cursor", async () => {
    apiFetchMock.mockResolvedValue(
      response({
        items: [
          {
            id: "server-tx-1",
            type: "yield",
            status: "completed",
            amount: "12.5",
            asset: "USDC",
            address: "yield.zaps",
            timestamp: "2026-07-20T10:00:00Z",
            yield_type: "interest",
          },
        ],
        next_cursor: "server-tx-1",
        total: 4,
      })
    );

    const page = await fetchTransactions(
      {
        ...baseFilters,
        yieldOnly: true,
        asset: "USDC",
        dateFrom: "2026-07-01",
      },
      "prior-cursor"
    );

    expect(apiFetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/api/transactions/history?")
    );
    const requestUrl = new URL(apiFetchMock.mock.calls[0][0]);
    expect(requestUrl.searchParams.get("type")).toBe("yield");
    expect(requestUrl.searchParams.get("asset")).toBe("USDC");
    expect(requestUrl.searchParams.get("date_from")).toBe("2026-07-01");
    expect(requestUrl.searchParams.get("cursor")).toBe("prior-cursor");
    expect(page.items.map((tx) => tx.id)).toEqual(["server-tx-1"]);
    expect(page.nextCursor).toBe("server-tx-1");
    expect(page.offlineCache).toBe(false);
  });

  it("does not substitute local mock transactions for an empty server page", async () => {
    apiFetchMock.mockResolvedValue(
      response({ items: [], next_cursor: null, total: 0 })
    );

    const page = await fetchTransactions(baseFilters, null);

    expect(page.items).toEqual([]);
    expect(page.total).toBe(0);
    expect(apiFetchMock).toHaveBeenCalledTimes(1);
  });

  it("serves only cached server responses during a network failure", async () => {
    apiFetchMock.mockResolvedValue(
      response({
        items: [
          {
            id: "server-cached",
            type: "received",
            status: "completed",
            amount: "3.25",
            asset: "USDC",
            address: "GSERVER",
            timestamp: "2026-07-20T10:00:00Z",
          },
        ],
        next_cursor: null,
        total: 1,
      })
    );
    const now = Date.now();
    jest.spyOn(Date, "now").mockReturnValue(now);
    await fetchTransactions(baseFilters, null);

    jest.spyOn(Date, "now").mockReturnValue(now + 31_000);
    apiFetchMock.mockRejectedValue(new TypeError("Network request failed"));
    const page = await fetchTransactions(baseFilters, null);

    expect(page.items.map((tx) => tx.id)).toEqual(["server-cached"]);
    expect(page.offlineCache).toBe(true);
  });

  it("returns no transaction detail for an unknown server ID", async () => {
    apiFetchMock.mockResolvedValue({ ok: false, status: 404 });

    await expect(fetchTransactionById("unknown-id")).resolves.toBeNull();
    expect(apiFetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/api/transactions/history/unknown-id")
    );
  });
});
