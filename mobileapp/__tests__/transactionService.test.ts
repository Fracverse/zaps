jest.mock("@react-native-async-storage/async-storage", () =>
  require("@react-native-async-storage/async-storage/jest/async-storage-mock")
);

jest.mock("../src/services/api", () => ({
  apiFetch: jest.fn(),
}));

import AsyncStorage from "@react-native-async-storage/async-storage";
import { apiFetch } from "../src/services/api";
import {
  fetchTransactions,
  fetchTransactionById,
  invalidateTransactionCache,
} from "../src/services/transactionService";

const mockedApiFetch = apiFetch as jest.MockedFunction<typeof apiFetch>;
const baseFilters = {
  type: "all" as const,
  status: "all" as const,
  search: "",
};

describe("transactionService", () => {
  beforeEach(async () => {
    jest.clearAllMocks();
    await invalidateTransactionCache();
  });

  it("uses authenticated API page results", async () => {
    mockedApiFetch.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        items: [
          {
            id: "server-1",
            type: "received",
            status: "completed",
            amount: "42.00",
            asset: "USDC",
            address: "GSERVER",
            timestamp: "2026-11-21T12:00:00Z",
          },
        ],
        next_cursor: "2026-11-21T12:00:00Z",
        total: 1,
      }),
    } as Response);

    const page = await fetchTransactions(baseFilters, null);

    expect(page.items.map((item) => item.id)).toEqual(["server-1"]);
    expect(mockedApiFetch).toHaveBeenCalledWith(
      expect.stringContaining("/api/transactions")
    );
  });

  it("returns an empty API page and never exposes mock transactions", async () => {
    mockedApiFetch.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [], next_cursor: null, total: 0 }),
    } as Response);

    const page = await fetchTransactions(baseFilters, null);

    expect(page.items).toEqual([]);
    expect(await AsyncStorage.getItem("tx_cache_v1")).not.toContain("tx_001");
  });

  it("throws API errors and never falls back to a mock detail", async () => {
    mockedApiFetch.mockResolvedValue({
      ok: false,
      status: 500,
      json: async () => ({ error: "Transaction service unavailable" }),
    } as Response);

    await expect(fetchTransactionById("missing")).rejects.toThrow(
      "Transaction service unavailable"
    );
  });

  it("throws list API errors without returning the mock list", async () => {
    mockedApiFetch.mockResolvedValue({
      ok: false,
      status: 503,
      json: async () => ({ error: "History unavailable" }),
    } as Response);

    await expect(fetchTransactions(baseFilters, null)).rejects.toThrow(
      "History unavailable"
    );
  });
});
