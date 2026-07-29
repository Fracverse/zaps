export type TransactionType =
  | "sent"
  | "received"
  | "swap"
  | "payment"
  | "yield"
  | "mass_payout";
export type TransactionStatus = "completed" | "pending" | "failed";
export type YieldType = "interest" | "reward" | "apy";

export interface Transaction {
  id: string;
  type: TransactionType;
  status: TransactionStatus;
  amount: string;
  asset: string;
  fiatValue: string;
  fiatCurrency: string;
  address: string;
  addressLabel?: string;
  timestamp: string;
  stellarTxHash?: string;
  memo?: string;
  fee?: string;
  feeAsset?: string;
  network?: string;
  yieldType?: YieldType;
  /** Populated for mass_payout (SDP disbursement) transactions */
  senderName?: string;
  /** Anchor/organisation that issued the SDP disbursement */
  senderAnchor?: string;
}

export interface TransactionPage {
  items: Transaction[];
  nextCursor: string | null;
  total: number;
}

export interface TransactionFilters {
  type: "all" | TransactionType;
  status: "all" | TransactionStatus;
  dateFrom?: string;
  dateTo?: string;
  search: string;
  amountMin?: string;
  amountMax?: string;
  yieldOnly?: boolean;
}
