import AsyncStorage from "@react-native-async-storage/async-storage";
import { fetchTransactions } from "../src/services/transactionService";
import type { TransactionFilters } from "../src/types/transaction";
import { activeYieldTag, toggleYieldTag } from "../src/utils/yieldFilter";

jest.mock("@react-native-async-storage/async-storage", () => {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  return require("@react-native-async-storage/async-storage/jest/async-storage-mock");
});

const baseFilters: TransactionFilters = {
  type: "all",
  status: "all",
  search: "",
};

describe("yield filter tags (#905)", () => {
  it("reports no tag when no yield filter is applied", () => {
    expect(activeYieldTag(baseFilters)).toBeNull();
  });

  it("selects the all-yield tag via yieldOnly", () => {
    const next = toggleYieldTag(baseFilters, "yield");
    expect(next.yieldOnly).toBe(true);
    expect(next.yieldType).toBeUndefined();
    expect(activeYieldTag(next)).toBe("yield");
  });

  it("selects a specific yield type tag", () => {
    const next = toggleYieldTag(baseFilters, "reward");
    expect(next.yieldOnly).toBe(true);
    expect(next.yieldType).toBe("reward");
    expect(activeYieldTag(next)).toBe("reward");
  });

  it("switches directly between tags", () => {
    const interest = toggleYieldTag(baseFilters, "interest");
    const apy = toggleYieldTag(interest, "apy");
    expect(activeYieldTag(apy)).toBe("apy");
  });

  it("clears the filter when the active tag is tapped again", () => {
    const next = toggleYieldTag(toggleYieldTag(baseFilters, "apy"), "apy");
    expect(activeYieldTag(next)).toBeNull();
    expect(next.yieldOnly).toBe(false);
  });

  it("keeps unrelated filters when toggling a tag", () => {
    const next = toggleYieldTag({ ...baseFilters, asset: "USDC" }, "interest");
    expect(next.asset).toBe("USDC");
  });
});

describe("yield type filtering in fetchTransactions (#905)", () => {
  beforeEach(async () => {
    await AsyncStorage.clear();
  });

  it.each(["interest", "reward", "apy"] as const)(
    "returns only %s yield transactions",
    async (yieldType) => {
      const page = await fetchTransactions(
        { ...baseFilters, yieldOnly: true, yieldType },
        null
      );
      expect(page.items.length).toBeGreaterThan(0);
      expect(
        page.items.every(
          (tx) => tx.type === "yield" && tx.yieldType === yieldType
        )
      ).toBe(true);
    }
  );

  it("treats a yield type as a yield filter even without yieldOnly", async () => {
    const page = await fetchTransactions(
      { ...baseFilters, yieldType: "interest" },
      null
    );
    expect(page.items.length).toBeGreaterThan(0);
    expect(page.items.every((tx) => tx.type === "yield")).toBe(true);
  });
});
