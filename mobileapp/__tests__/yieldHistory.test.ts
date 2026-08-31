import AsyncStorage from "@react-native-async-storage/async-storage";
import { fetchTransactions } from "../src/services/transactionService";
import { getYieldTransactions } from "../src/services/api";
import type { TransactionFilters } from "../src/types/transaction";

jest.mock("@react-native-async-storage/async-storage", () => {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  return require("@react-native-async-storage/async-storage/jest/async-storage-mock");
});

const baseFilters: TransactionFilters = {
  type: "all",
  status: "all",
  search: "",
};

const last30Days = new Date(Date.now() - 30 * 24 * 60 * 60 * 1000)
  .toISOString()
  .slice(0, 10);

describe("yield transaction history filtering (#692)", () => {
  beforeEach(async () => {
    await AsyncStorage.clear();
  });

  it("filters the client-side list by asset type", async () => {
    const page = await fetchTransactions(
      { ...baseFilters, yieldOnly: true, asset: "USDC" },
      null
    );
    expect(page.items.length).toBeGreaterThan(0);
    expect(page.items.every((tx) => tx.asset.toUpperCase() === "USDC")).toBe(
      true
    );
  });

  it("filters the client-side list by date range", async () => {
    const page = await fetchTransactions(
      {
        ...baseFilters,
        yieldOnly: true,
        dateFrom: last30Days,
        dateTo: undefined,
      },
      null
    );
    const cutoff = new Date(`${last30Days}T00:00:00`);
    expect(page.items.every((tx) => new Date(tx.timestamp) >= cutoff)).toBe(
      true
    );
  });

  it("refreshes yield history via api.getYieldTransactions with params", async () => {
    const items = await getYieldTransactions({
      asset: "XLM",
      dateFrom: last30Days,
    });
    expect(items.every((tx) => tx.type === "yield")).toBe(true);
    expect(items.every((tx) => tx.asset.toUpperCase() === "XLM")).toBe(true);
  });

  it("yield-only filter never returns non-yield transactions", async () => {
    const all = await fetchTransactions(baseFilters, null);
    const nonYield = all.items.filter((tx) => tx.type !== "yield");
    const page = await fetchTransactions(
      { ...baseFilters, yieldOnly: true },
      null
    );
    expect(page.items.every((tx) => tx.type === "yield")).toBe(true);
    expect(page.items.length).toBeLessThanOrEqual(
      all.items.length - nonYield.length
    );
  });
});
