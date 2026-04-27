export type TransactionType = "sent" | "received" | "swap" | "payment";
export type TransactionStatus = "completed" | "pending" | "failed";

export interface Transaction {
  id: string;
  type: TransactionType;
  status: TransactionStatus;
  amount: string; // e.g. "12.50"
  asset: string; // e.g. "USDC"
  fiatValue: string; // e.g. "12.50"
  fiatCurrency: string; // e.g. "USD"
  address: string; // counterparty full address
  addressLabel?: string; // optional human-readable label / blink ID
  timestamp: string; // ISO 8601
  stellarTxHash?: string;
  memo?: string;
  fee?: string;
  feeAsset?: string;
  network?: string;
}

export interface TransactionPage {
  items: Transaction[];
  nextCursor: string | null;
  total: number;
}

export interface TransactionFilters {
  type: "all" | TransactionType;
  status: "all" | TransactionStatus;
  dateFrom?: string; // ISO date string
  dateTo?: string;
  search: string;
  amountMin?: string;
  amountMax?: string;
}
