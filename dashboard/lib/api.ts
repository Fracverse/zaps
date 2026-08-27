const BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080";
const SERVER_BASE =
  process.env.NEXT_PUBLIC_SERVER_URL ?? "http://localhost:3000";
/** Stellar Disbursement Platform admin API base URL */
const SDP_BASE =
  process.env.NEXT_PUBLIC_SDP_URL ?? "http://localhost:8000";

export const TOKEN_KEY = "token";
export const REFRESH_TOKEN_KEY = "refresh_token";

/** Read a stored credential, tolerating a non-browser (SSR) context. */
function readStored(key: string): string | null {
  return typeof window !== "undefined" ? localStorage.getItem(key) : null;
}

export function getToken(): string | null {
  return readStored(TOKEN_KEY);
}

export function getRefreshToken(): string | null {
  return readStored(REFRESH_TOKEN_KEY);
}

/** Persist the credentials returned by login or refresh. */
export function storeSession(token: string, refreshToken?: string): void {
  if (typeof window === "undefined") return;
  localStorage.setItem(TOKEN_KEY, token);
  if (refreshToken) localStorage.setItem(REFRESH_TOKEN_KEY, refreshToken);
  document.cookie = `${TOKEN_KEY}=${encodeURIComponent(token)}; path=/; SameSite=Lax`;
}

export function clearSession(): void {
  if (typeof window === "undefined") return;
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(REFRESH_TOKEN_KEY);
  document.cookie = `${TOKEN_KEY}=; path=/; max-age=0`;
  document.cookie = `zaps-auth=; path=/; max-age=0`;
}

/**
 * A single in-flight refresh, shared by every request that hits a 401 at once.
 *
 * A dashboard page fires several requests on mount. When the token expires
 * they all come back 401 together, and without this each would spend the
 * refresh token separately — the backend rotates it, so the first refresh
 * invalidates the rest and the user is logged out despite holding a perfectly
 * good session.
 */
let refreshInFlight: Promise<string | null> | null = null;

/**
 * Exchange the stored refresh token for a new access token.
 *
 * Resolves to the new access token, or null when there is nothing to exchange
 * or the server rejects it. Never throws: callers treat null as "could not
 * refresh" and surface the original error.
 */
export async function refreshAccessToken(): Promise<string | null> {
  if (refreshInFlight) return refreshInFlight;

  const refresh_token = getRefreshToken();
  if (!refresh_token) return null;

  refreshInFlight = (async () => {
    try {
      const res = await fetch(`${BASE}/auth/refresh`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ refresh_token }),
      });

      if (!res.ok) {
        // The refresh token is spent or revoked — nothing left to try.
        clearSession();
        return null;
      }

      const data = (await res.json()) as {
        token?: string;
        refresh_token?: string;
      };

      if (!data.token) {
        clearSession();
        return null;
      }

      // The backend rotates refresh tokens, so store whatever came back.
      storeSession(data.token, data.refresh_token);
      return data.token;
    } catch {
      // Network failure: leave the session alone so a reload can retry.
      return null;
    } finally {
      refreshInFlight = null;
    }
  })();

  return refreshInFlight;
}

/** Build request headers, stamping the bearer token when there is one. */
function authHeaders(
  token: string | null,
  init?: RequestInit,
  base: Record<string, string> = { "Content-Type": "application/json" },
): HeadersInit {
  return {
    ...base,
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
    ...(init?.headers as Record<string, string> | undefined),
  };
}

/**
 * Perform an authenticated request, refreshing the access token once and
 * retrying if the server answers 401.
 *
 * The retry is deliberately single-shot: if the replayed request is rejected
 * too, the token is not the problem and looping would just multiply the
 * failure.
 */
async function authedFetch(
  url: string,
  init: RequestInit | undefined,
  buildHeaders: (token: string | null) => HeadersInit,
): Promise<Response> {
  const res = await fetch(url, { ...init, headers: buildHeaders(getToken()) });
  if (res.status !== 401) return res;

  const token = await refreshAccessToken();
  if (!token) return res;

  return fetch(url, { ...init, headers: buildHeaders(token) });
}

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await authedFetch(`${BASE}${path}`, init, (token) =>
    authHeaders(token, init),
  );
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
  // Batch payouts (BE-554)
  listBatchPayouts: (limit = 20, offset = 0) =>
    req<{ batches: BatchPayout[]; total: number }>(
      `/api/payouts/batches?limit=${limit}&offset=${offset}`,
    ),
  getBatchPayout: (id: string) =>
    req<{ batch: BatchPayout; recipients: BatchRecipient[] }>(
      `/api/payouts/batch/${id}`,
    ),
  /** Retry a single failed recipient in a batch disbursement. */
  retryBatchItem: (id: string, itemIndex: number) =>
    serverReq<{ status?: string; recipient?: BatchRecipient }>(
      `/api/v1/payouts/batch/${id}/retry`,
      {
        method: "POST",
        body: JSON.stringify({ item_index: itemIndex }),
      },
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

  // Contract config (fee coefficient)
  contractConfig: () =>
    serverReq<ContractConfig>("/api/v1/admin/contracts/config"),

  setFeeCoefficient: (fee_coefficient: number) =>
    serverReq<{ fee_coefficient: number; tx_hash: string }>(
      "/api/v1/admin/contracts/config/fee-coefficient",
      {
        method: "POST",
        body: JSON.stringify({ fee_coefficient }),
      },
    ),

  // Yield vault aggregate metrics
  yieldStats: () => req<YieldStats>("/admin/vault/stats"),

  // Yield vault APY rate history
  yieldRateHistory: () =>
    req<{ rates: { apy: number; created_at: string }[] }>("/api/yield/rates/history"),

  // Username registry
  searchUsers: (query: string) =>
    req<UserSearchResult[]>(`/api/users/search?q=${encodeURIComponent(query)}`),

  registryClaims: () =>
    req<RegistryClaim[]>("/api/registry/claims"),

  registryStats: () =>
    req<RegistryStats>("/api/registry/stats"),

  userPayments: (username: string) =>
    req<SocialFeedItem[]>(`/api/users/${encodeURIComponent(username)}/payments`),

  // ── Identity linking (Privy DID ↔ Stellar address) ────────────────────────
  /** Fetch the full identity link table from the backend admin API. */
  identityLinks: (params?: { query?: string; limit?: number; offset?: number }) => {
    const qs = new URLSearchParams();
    if (params?.query) qs.set("q", params.query);
    if (params?.limit != null) qs.set("limit", String(params.limit));
    if (params?.offset != null) qs.set("offset", String(params.offset));
    const suffix = qs.toString() ? `?${qs.toString()}` : "";
    return req<{ links: IdentityLink[]; total: number }>(
      `/admin/identity/links${suffix}`
    );
  },

  /** Fetch a single identity link by user ID. */
  getIdentityLink: (userId: string) =>
    req<IdentityLink>(`/admin/identity/links/${encodeURIComponent(userId)}`),

  // ── Admin audit log (#797) ────────────────────────────────────────────────
  /** Fetch admin audit log entries, descending by timestamp. */
  adminLogs: (limit = 50, offset = 0) =>
    serverReq<{ logs: AdminAuditLog[]; total: number }>(
      `/api/v1/admin/logs?limit=${limit}&offset=${offset}`,
    ),

  // ── Backend health (#785) ─────────────────────────────────────────────────
  /** Poll the backend /health endpoint. No auth required. */
  health: () =>
    fetch(`${BASE}/health`)
      .then((res) => res.json() as Promise<HealthResponse>),

  // ── SDP (Stellar Disbursement Platform) ───────────────────────────────────
  sdp: {
    /**
     * Upload a CSV file to create a new SDP disbursement batch.
     * The Authorization header carries the admin Bearer token;
     * SDP-Admin-Token provides the platform-level credential.
     */
    uploadDisbursementCSV: (file: File, disbursementName: string) => {
      const form = new FormData();
      form.append("file", file);
      form.append("disbursement_name", disbursementName);
      return sdpReq<SdpDisbursement>("/api/disbursements", {
        method: "POST",
        body: form,
        // Content-Type is intentionally omitted so the browser sets the
        // multipart boundary automatically.
        headers: {},
      });
    },

    /** List all disbursements with optional pagination. */
    listDisbursements: (limit = 20, offset = 0) =>
      sdpReq<{ disbursements: SdpDisbursement[]; total: number }>(
        `/api/disbursements?limit=${limit}&offset=${offset}`
      ),

    /** Fetch a single disbursement by ID. */
    getDisbursement: (id: string) =>
      sdpReq<SdpDisbursement>(`/api/disbursements/${id}`),

    /**
     * Retrieve execution logs for a disbursement.
     * Logs include status transitions, errors, and blockchain confirmations.
     */
    getDisbursementLogs: (id: string) =>
      sdpReq<{ logs: SdpExecutionLog[] }>(
        `/api/disbursements/${id}/logs`
      ),
  },
};

async function serverReq<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await authedFetch(`${SERVER_BASE}${path}`, init, (token) =>
    authHeaders(token, init),
  );
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  return res.json();
}

/**
 * Fetch helper for the Stellar Disbursement Platform admin API.
 *
 * Request headers follow the SDP convention:
 *   Authorization: Bearer <admin-jwt>          (from localStorage "token")
 *   SDP-Admin-Token: <sdp-static-admin-token>  (from env NEXT_PUBLIC_SDP_ADMIN_TOKEN)
 *
 * For multipart requests (CSV upload) the caller should pass an empty or
 * partial `headers` object so that Content-Type is NOT set — the browser
 * fills in the correct `multipart/form-data; boundary=…` value automatically.
 */
async function sdpReq<T>(path: string, init?: RequestInit): Promise<T> {
  const sdpAdminToken = process.env.NEXT_PUBLIC_SDP_ADMIN_TOKEN ?? "";
  const isMultipart = init?.body instanceof FormData;

  const res = await authedFetch(`${SDP_BASE}${path}`, init, (token) => ({
    // Only set Content-Type for JSON requests; let browser handle multipart.
    ...authHeaders(token, undefined, isMultipart ? {} : { "Content-Type": "application/json" }),
    ...(sdpAdminToken ? { "SDP-Admin-Token": sdpAdminToken } : {}),
    ...(init?.headers as Record<string, string> | undefined),
  }));
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

export interface BatchPayout {
  id: string;
  status: string;
  currency: string;
  total_recipients: number;
  total_amount: number;
  succeeded_count: number;
  failed_count: number;
  created_at: string;
  started_at?: string;
  completed_at?: string;
}

export interface BatchRecipient {
  id: string;
  user_id?: string;
  destination_address?: string;
  amount: number;
  status: string;
  tx_hash?: string;
  attempt_count: number;
  created_at: string;
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

export interface ContractConfig {
  fee_coefficient: number;
}

export interface YieldStats {
  total_value_locked: number;
  total_yield_distributed: number;
  apy: number;
}

export interface UserSearchResult {
  username: string;
  public_key: string;
  registered_at: string;
}

export interface RegistryClaim {
  username: string;
  public_key: string;
  registered_at: string;
  tx_hash?: string;
}

export interface RegistryStats {
  total_usernames: number;
  weekly_growth: number;
  active_registrations: number;
}

// ── Backend health (#785) ───────────────────────────────────────────────────

export interface HealthComponentStatus {
  status: string;
  latency_ms: number;
}

export interface HealthResponse {
  /** "ok" when all components are healthy; "degraded" otherwise. */
  status: "ok" | "degraded";
  components: {
    database: HealthComponentStatus;
    yield_db: HealthComponentStatus;
    soroban_rpc: HealthComponentStatus;
  };
  checked_at: string;
}

// ── Identity linking ────────────────────────────────────────────────────────

/**
 * Maps a Privy DID (did:privy:…) to a Stellar public address.
 * Returned by the backend admin identity API.
 */
export interface IdentityLink {
  /** Internal user ID (UUID). */
  user_id: string;
  /** Privy decentralised identifier string, e.g. did:privy:clxxxxxxxxxxx */
  privy_did: string;
  /** Stellar public key (G-address, 56 chars). */
  stellar_address: string;
  /** Display name or username, if available. */
  display_name?: string;
  /** Email associated with the Privy account, if available. */
  email?: string;
  /** ISO-8601 timestamp of when the link was created. */
  linked_at: string;
  /** Verification status of the identity link. */
  status: "active" | "pending" | "revoked";
}

// ── Admin audit log ────────────────────────────────────────────────────────

/** A single admin action recorded in the server-side audit log. */
export interface AdminAuditLog {
  /** Unique log entry ID. */
  id: string;
  /** Admin user ID who performed the action. */
  admin_id: string;
  /** Human-readable action label, e.g. "fee_coefficient_updated". */
  action: string;
  /** Free-form details or JSON payload diff. */
  details?: string;
  /** IP address of the requesting admin. */
  ip_address?: string;
  /** ISO-8601 timestamp of the action. */
  timestamp: string;
}

// ── Stellar Disbursement Platform (SDP) ────────────────────────────────────

/** A disbursement batch managed by the Stellar Disbursement Platform. */
export interface SdpDisbursement {
  id: string;
  name: string;
  status:
    | "DRAFT"
    | "READY"
    | "STARTED"
    | "PAUSED"
    | "COMPLETED"
    | "FAILED";
  asset_code: string;
  asset_issuer?: string;
  total_payments: number;
  successful_payments: number;
  failed_payments: number;
  cancelled_payments: number;
  remaining_payments: number;
  total_amount: string;
  disbursed_amount: string;
  created_at: string;
  updated_at: string;
  started_at?: string;
  completed_at?: string;
  wallet?: { name: string; homepage: string };
}

/**
 * A single execution log entry for an SDP disbursement.
 * Logs capture status transitions, RPC confirmations, and any errors.
 */
export interface SdpExecutionLog {
  id: string;
  disbursement_id: string;
  level: "info" | "warning" | "error";
  message: string;
  metadata?: Record<string, unknown>;
  created_at: string;
}
