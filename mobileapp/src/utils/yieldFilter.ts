import { TransactionFilters, YieldType } from "../types/transaction";

/** "yield" matches every yield update; the rest narrow to one yield type. */
export type YieldFilterTag = "yield" | YieldType;

export const YIELD_FILTER_TAGS: { tag: YieldFilterTag; label: string }[] = [
  { tag: "yield", label: "All yield" },
  { tag: "interest", label: "Interest" },
  { tag: "reward", label: "Reward" },
  { tag: "apy", label: "APY" },
];

/** The yield tag currently applied by `filters`, or null when none is. */
export function activeYieldTag(
  filters: TransactionFilters
): YieldFilterTag | null {
  if (filters.yieldType) return filters.yieldType;
  return filters.yieldOnly ? "yield" : null;
}

/** Select `tag`, or clear the yield filter when `tag` is already selected. */
export function toggleYieldTag(
  filters: TransactionFilters,
  tag: YieldFilterTag
): TransactionFilters {
  if (activeYieldTag(filters) === tag) {
    return { ...filters, yieldOnly: false, yieldType: undefined };
  }
  return {
    ...filters,
    yieldOnly: true,
    yieldType: tag === "yield" ? undefined : tag,
  };
}
