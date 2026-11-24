// Currency display helper for the home screen balance card.
//
// Balances are sourced from the yield vault as XLM token amounts. The Naira
// figure is always derived — fiat = tokens * XLM→NGN rate — so the same raw
// value can be shown as either an XLM token balance or its ₦ equivalent by
// simply swapping the formatting.
export type BalanceCurrency = "NGN" | "XLM";

// Fallback XLM→NGN conversion rate used when rendering the fiat equivalent.
//
// Deliberately a single named constant rather than a magic number scattered
// across the screen, and deliberately a static value until a live FX feed is
// wired up. The backend already plans to echo the source of its own rate
// (`fx_rate_source` in `backend/src/api/yield.rs`); the long-term answer is to
// surface that same rate here instead of compiling one in.
export const XLM_TO_NGN_RATE = 1600;

// Formats a whole XLM balance as a token amount, e.g. `1,234.5 XLM`.
export const formatTokenBalance = (xlm: number): string =>
  `${xlm.toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 6,
  })} XLM`;

// Formats a Naira value, e.g. `₦1,234.56`.
export const formatFiatBalance = (ngn: number): string =>
  `₦${ngn.toLocaleString("en-NG", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;

// Converts an XLM token balance into its Naira equivalent.
export const fiatFromTokens = (
  xlm: number,
  rate: number = XLM_TO_NGN_RATE
): number => xlm * rate;

// Formats an XLM token balance in the requested denomination.
//
// - "XLM" shows the raw token amount.
// - "NGN" multiplies by the current rate (fiat equivalent).
export const formatBalance = (
  xlm: number,
  currency: BalanceCurrency,
  rate: number = XLM_TO_NGN_RATE
): string => {
  if (currency === "XLM") return formatTokenBalance(xlm);
  return formatFiatBalance(fiatFromTokens(xlm, rate));
};
