import type { YieldAuditLogItem } from "./api";

export interface YieldTx {
  id: string;
  /** Empty while `pending` — never synthesized. */
  txHash: string;
  timestamp: string;
  /** Full Stellar public key of the address involved. */
  address: string;
  action: "deposit" | "withdraw" | "yield_accrual" | "config";
  tokenVolume: number;
  asset: string;
  blockHeight?: number;
  fee?: number;
  yieldIndex?: number;
  details?: string;
  /** Submitted locally, not yet seen in the audit API. */
  pending?: boolean;
}

function mapAction(type: string): YieldTx["action"] {
  const t = type.toLowerCase();
  if (t.includes("deposit")) return "deposit";
  if (t.includes("withdraw")) return "withdraw";
  if (t.includes("config")) return "config";
  return "yield_accrual";
}

export function mapAuditItem(item: YieldAuditLogItem): YieldTx {
  return {
    id: item.id,
    txHash: item.tx_hash,
    timestamp: item.created_at,
    address: item.address ?? "",
    action: mapAction(item.type),
    tokenVolume: Math.abs(item.amount) / 10_000_000,
    asset: item.asset || "—",
    blockHeight: item.block_height,
    fee: item.fee,
  };
}

/**
 * Next audit list after a query. API rows replace everything that came from
 * the API before; only locally submitted, still-pending entries are kept, and
 * dropped once the API returns the same tx hash. `items === null` means the
 * query failed: nothing but pending entries survives.
 */
export function reconcileAuditRows(
  prev: YieldTx[],
  items: YieldAuditLogItem[] | null,
): YieldTx[] {
  const mapped = (items ?? []).map(mapAuditItem);
  const confirmed = new Set(mapped.map((m) => m.txHash));
  const pending = prev.filter((p) => p.pending && !confirmed.has(p.txHash));
  return [...pending, ...mapped];
}
