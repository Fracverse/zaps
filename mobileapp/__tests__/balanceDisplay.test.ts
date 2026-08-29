import {
  XLM_TO_NGN_RATE,
  fiatFromTokens,
  formatBalance,
  formatFiatBalance,
  formatTokenBalance,
} from "../src/utils/balanceDisplay";

describe("balance display currency helper", () => {
  it("converts token balances to a Naira equivalent by multiplying the rate", () => {
    expect(fiatFromTokens(2, 1600)).toBe(3200);
    expect(fiatFromTokens(0)).toBe(0);
    expect(fiatFromTokens(1.5)).toBeCloseTo(1.5 * XLM_TO_NGN_RATE);
  });

  it("formats token balances with the XLM suffix", () => {
    expect(formatTokenBalance(2)).toBe("2.00 XLM");
    expect(formatTokenBalance(0)).toBe("0.00 XLM");
  });

  it("formats Naira figures with the currency symbol and two decimals", () => {
    expect(formatFiatBalance(3200)).toBe("₦3,200.00");
    expect(formatFiatBalance(0)).toBe("₦0.00");
  });

  it("shows the fiat equivalent when the NGN toggle is active", () => {
    expect(formatBalance(2, "NGN")).toBe("₦3,200.00");
  });

  it("shows the raw token amount when the XLM toggle is active", () => {
    expect(formatBalance(2, "XLM")).toBe("2.00 XLM");
  });

  it("defaults to the configured rate but honors an explicit one", () => {
    expect(formatBalance(2, "NGN")).toBe("₦3,200.00");
    expect(formatBalance(2, "NGN", 100)).toBe("₦200.00");
  });

  it("handles fractional token balances without precision loss", () => {
    expect(formatBalance(0.145, "XLM")).toBe("0.145 XLM");
  });
});