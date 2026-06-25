const BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080";
const SERVER_BASE =
  process.env.NEXT_PUBLIC_SERVER_URL ?? "http://localhost:3000";

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const token =
    typeof window !== "undefined" ? localStorage.getItem("token") : null;
  const res = await fetch(`${BASE}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...init?.headers,
    },
  });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
}

export const api = {
  // Auth — backend uses user_id + PIN (4–6 digits), not email/password
  login: (user_id: string, pin: string) =>
    req<{
      token: string;
      refresh_token: string;
      user_id: string;
      role: string;
    }>("/auth/login", {
      method: "POST",
      body: JSON.stringify({ user_id, pin }),
    }),

  // Admin dashboard stats
  dashboardStats: () =>
    req<{
      total_users: number;
      total_payments: number;
      total_transfers: number;
      total_withdrawals: number;
      active_merchants: number;
    }>("/admin/dashboard/stats"),

  // Social payment feed
  socialFeed: (limit = 100) =>
    req<SocialFeedItem[]>(`/api/feed/public?limit=${limit}&offset=0`),

  // Transactions
  transactions: (params?: Record<string, string>) => {
    const qs = params ? "?" + new URLSearchParams(params).toString() : "";
    return req<Transaction[]>(`/admin/transactions${qs}`);
  },

  // Payments
  getPayment: (id: string) => req<Payment>(`/payments/${id}`),
  generateQr: (body: QrRequest) =>
    req<{ qr_data: string; xdr_payload?: string }>("/qr/generate", {
      method: "POST",
      body: JSON.stringify(body),
    }),

  // Withdrawals / Payouts
  createWithdrawal: (body: WithdrawalRequest) =>
    req<Withdrawal>("/withdrawals", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  getWithdrawal: (id: string) => req<Withdrawal>(`/withdrawals/${id}`),

  // Payouts (Node server)
  requestPayout: (body: PayoutRequest) =>
    req<{ payout: Payout }>("/payouts", {
      method: "POST",
      body: JSON.stringify(body),
    }),
  payoutHistory: (limit = 20, offset = 0) =>
    req<{ payouts: Payout[] }>(
      `/payouts/history?limit=${limit}&offset=${offset}`,
    ),

  // Profile
  myProfile: () => req<Profile>("/profiles/me"),

  // Contract monitoring (Node server)
  contractHealth: () =>
    serverReq<ContractHealthResponse>("/api/v1/admin/contracts/health"),

  contractMetrics: () =>
    serverReq<ContractMetricsResponse>("/api/v1/admin/contracts/metrics"),

  contractAlerts: () =>
    serverReq<{ alerts: ContractAlert[] }>("/api/v1/admin/contracts/alerts"),
};

async function serverReq<T>(path: string, init?: RequestInit): Promise<T> {
  const token =
    typeof window !== "undefined" ? localStorage.getItem("token") : null;
  const res = await fetch(`${SERVER_BASE}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...init?.headers,
    },
  });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
}

// ── Types ──────────────────────────────────────────────────────────────────────

export interface Transaction {
  id: string;
  tx_hash?: string;
  from_address: string;
  merchant_id: string;
  send_asset: string;
  send_amount: number;
  receive_amount?: number;
  status: "pending" | "processing" | "completed" | "failed" | "refunded";
  memo?: string;
  created_at: string;
}

export interface SocialFeedItem {
  id: string;
  tx_hash: string;
  sender_username: string;
  sender_avatar?: string;
  receiver_username: string;
  receiver_avatar?: string;
  amount: string;
  currency: string;
  memo: string;
  likes_count: number;
  comments_count: number;
  has_liked: boolean;
  created_at: string;
  visibility: "PUBLIC" | "FRIENDS" | "PRIVATE";
}

export type Payment = Transaction;

export interface Withdrawal {
  id: string;
  user_id: string;
  destination_address: string;
  amount: number;
  asset: string;
  status: string;
  anchor_tx_id?: string;
  kyc_status: string;
  sep24_interactive_url?: string;
  created_at: string;
}

export interface Payout {
  id: string;
  merchantId: string;
  amount: string;
  asset: string;
  status: string;
  bankAccountId: string;
  anchorId: string;
  createdAt: string;
}

export interface Profile {
  id: string;
  user_id: string;
  display_name?: string;
  avatar_url?: string;
}

export interface QrRequest {
  merchant_id: string;
  amount: number;
  asset: string;
  memo?: string;
  expiry: number;
}

export interface WithdrawalRequest {
  destination_address: string;
  amount: number;
  asset: string;
}

export interface PayoutRequest {
  amount: string;
  asset: string;
  bankAccountId: string;
  anchorId: string;
}

export interface ContractHealthStatus {
  name: string;
  contractId: string;
  configured: boolean;
  reachable: boolean;
  paused?: boolean;
  lastChecked: string;
  error?: string;
}

export interface ContractHealthResponse {
  status: string;
  contracts: ContractHealthStatus[];
  sorobanRpc: string;
  latestLedger: number;
}

export interface ContractMetricsResponse {
  sorobanRpcLatencyMs: number;
  latestLedger: number;
  eventPollLagLedgers: number;
  lastEventPollAt: string | null;
  eventsTotal: {
    initiated: number;
    settled: number;
    failed: number;
    other: number;
  };
  simulationCount: number;
  simulationErrorCount: number;
  avgSimulationMs: number;
  uptimeSeconds: number;
}

export interface ContractAlert {
  id: string;
  severity: "info" | "warning" | "critical";
  title: string;
  message: string;
  metric: string;
  value: number;
  threshold: number;
  timestamp: string;
}
