/**
 * api.ts
 *
 * Central API client for the Zaps mobile app.
 *
 * #586 — Secure Store Privy Session Tokens
 *  - Privy access tokens and user-state payloads are stored in the device
 *    keychain / secure enclave via expo-secure-store (never in AsyncStorage).
 *  - `setSessionToken` / `getSessionToken` / `clearSessionToken` wrap all
 *    keychain I/O so callers never touch SecureStore directly.
 *  - Every outgoing request is decorated with `Authorization: Bearer <token>`
 *    by the `apiFetch` wrapper.
 *  - A 401 response automatically clears the cached session and invokes the
 *    optional `onSessionExpired` callback so the app can redirect to login.
 */

import AsyncStorage from "@react-native-async-storage/async-storage";
import * as SecureStore from "expo-secure-store";

// ── Config ────────────────────────────────────────────────────────────────────

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";

// ── Secure-store keys ─────────────────────────────────────────────────────────

/** Key under which the Privy JWT access token is stored in the keychain. */
const SESSION_TOKEN_KEY = "privy_session_token";

/** Key under which the serialised Privy user-state payload is stored. */
const SESSION_USER_STATE_KEY = "privy_user_state";

// ── #586 — Session token helpers ─────────────────────────────────────────────

/**
 * Persist a Privy access token (and optionally the full user-state payload)
 * into the device keychain / secure enclave.
 *
 * Storing the user state alongside the token lets the app re-hydrate the
 * Privy context on cold start without waiting for a network round-trip.
 */
export async function setSessionToken(
  token: string,
  userState?: object
): Promise<void> {
  await SecureStore.setItemAsync(SESSION_TOKEN_KEY, token);
  if (userState !== undefined) {
    await SecureStore.setItemAsync(
      SESSION_USER_STATE_KEY,
      JSON.stringify(userState)
    );
  }
}

/**
 * Read the cached Privy JWT token from the device keychain.
 * Returns `null` when no token has been stored yet or after `clearSessionToken`
 * has been called.
 */
export async function getSessionToken(): Promise<string | null> {
  try {
    return await SecureStore.getItemAsync(SESSION_TOKEN_KEY);
  } catch {
    // SecureStore can throw when the device has no passcode / biometric
    // enrolled. Treat as a cache miss and fall through to unauthenticated mode.
    return null;
  }
}

/**
 * Read the cached Privy user-state payload from the device keychain.
 * Returns `null` when none has been stored.
 */
export async function getSessionUserState<T = unknown>(): Promise<T | null> {
  try {
    const raw = await SecureStore.getItemAsync(SESSION_USER_STATE_KEY);
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

/**
 * Remove both the Privy access token and the user-state payload from the
 * device keychain.  Call this on logout or when a 401 is received.
 */
export async function clearSessionToken(): Promise<void> {
  await SecureStore.deleteItemAsync(SESSION_TOKEN_KEY).catch(() => {});
  await SecureStore.deleteItemAsync(SESSION_USER_STATE_KEY).catch(() => {});
}

// ── Session-expiry handler ────────────────────────────────────────────────────

/** Callback invoked whenever a 401 response triggers automatic session teardown. */
let _onSessionExpired: (() => void) | null = null;

/**
 * Register a callback that fires when the server returns 401.
 * Typically used by the root layout to navigate the user back to the login
 * screen without coupling the API layer to the router.
 *
 * @example
 * // In _layout.tsx
 * registerSessionExpiredHandler(() => router.replace("/login"));
 */
export function registerSessionExpiredHandler(handler: () => void): void {
  _onSessionExpired = handler;
}

// ── Core fetch wrapper ────────────────────────────────────────────────────────

/**
 * Authenticated fetch wrapper used by every API call in this module.
 *
 * - Reads the session token from SecureStore on each call (cheap; the OS
 *   caches keychain entries in memory after the first read).
 * - Injects `Authorization: Bearer <token>` when a token is present.
 * - On a 401 response, clears the stored session and fires the expiry handler
 *   so the app can redirect to login.
 */
async function apiFetch(
  input: string,
  init?: RequestInit
): Promise<Response> {
  const token = await getSessionToken();
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(init?.headers as Record<string, string>),
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  };

  const response = await fetch(input, { ...init, headers });

  if (response.status === 401) {
    // Session has expired or the token was revoked on the server.
    await clearSessionToken();
    _onSessionExpired?.();
  }

  return response;
}

// ── Recent recipients (unchanged; still uses AsyncStorage for non-sensitive data) ──

const RECENT_RECIPIENTS_KEY = "recent_recipient_usernames";
const MAX_RECENT_RECIPIENTS = 20;

export async function getRecentRecipients(): Promise<string[]> {
  try {
    const raw = await AsyncStorage.getItem(RECENT_RECIPIENTS_KEY);
    return raw ? (JSON.parse(raw) as string[]) : [];
  } catch {
    return [];
  }
}

export async function saveRecentRecipient(username: string): Promise<void> {
  if (!username) return;
  try {
    const raw = await AsyncStorage.getItem(RECENT_RECIPIENTS_KEY);
    const current = raw ? (JSON.parse(raw) as string[]) : [];
    const next = [
      username,
      ...current.filter((item) => item !== username),
    ].slice(0, MAX_RECENT_RECIPIENTS);
    await AsyncStorage.setItem(RECENT_RECIPIENTS_KEY, JSON.stringify(next));
  } catch {
    // Ignore cache failures.
  }
}

// ── Yield API ─────────────────────────────────────────────────────────────────

export interface YieldBalance {
  apy: string | number;
  totalYieldEarned: string | number;
  availableBalance: string | number;
  earningBalance: string | number;
  explanation: string;
  autoEarnEnabled: boolean;
}

export async function fetchYieldBalance(): Promise<YieldBalance> {
  const res = await apiFetch(`${API_BASE}/api/yield/balance`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json() as Promise<YieldBalance>;
}

export async function updateAutoEarn(enabled: boolean): Promise<void> {
  try {
    await apiFetch(`${API_BASE}/api/yield/auto-earn`, {
      method: "PATCH",
      body: JSON.stringify({ enabled }),
    });
  } catch {
    // Non-fatal — local state already reflects the toggle
  }
}

// ── Username Resolution ──────────────────────────────────────────────────────

export interface ReceiverDetails {
  username: string;
  address: string;
}

export interface UsernameLookupError extends Error {
  code: "NOT_FOUND" | "INVALID_USERNAME" | "NETWORK_ERROR" | "UNKNOWN_ERROR";
  statusCode?: number;
}

/**
 * Resolves a username to its registered Stellar address for transaction flows.
 * Uses the cached backend endpoint that checks Redis before falling back to Postgres.
 * Automatically saves successful lookups to recent recipients.
 * 
 * @param username - The username to resolve (must be 3-15 chars, lowercase alphanumeric)
 * @returns Promise resolving to receiver details with username and address
 * @throws UsernameLookupError with specific error codes for different failure scenarios
 */
export async function resolveUsername(username: string): Promise<ReceiverDetails> {
  if (!username || typeof username !== "string") {
    const error = new Error("Username is required and must be a string") as UsernameLookupError;
    error.code = "INVALID_USERNAME";
    throw error;
  }

  const trimmedUsername = username.trim().toLowerCase();
  if (!trimmedUsername) {
    const error = new Error("Username cannot be empty") as UsernameLookupError;
    error.code = "INVALID_USERNAME";
    throw error;
  }

  try {
    const response = await apiFetch(`${API_BASE}/api/users/resolve/${encodeURIComponent(trimmedUsername)}`);
    
    if (!response.ok) {
      const errorData = await response.json().catch(() => ({}));
      const error = new Error(
        errorData.error || `HTTP ${response.status}: ${response.statusText}`
      ) as UsernameLookupError;
      error.statusCode = response.status;
      
      if (response.status === 404) {
        error.code = "NOT_FOUND";
      } else if (response.status === 400) {
        error.code = "INVALID_USERNAME";
      } else {
        error.code = "NETWORK_ERROR";
      }
      
      throw error;
    }

    const data = await response.json();
    
    // Validate the response structure
    if (!data || typeof data !== "object" || !data.username || !data.address) {
      const error = new Error("Invalid response format from server") as UsernameLookupError;
      error.code = "UNKNOWN_ERROR";
      throw error;
    }

    const receiverDetails: ReceiverDetails = {
      username: data.username,
      address: data.address,
    };

    // Save to recent recipients on successful lookup
    await saveRecentRecipient(receiverDetails.username).catch(() => {
      // Ignore cache save failures - don't let them break the main flow
    });

    return receiverDetails;
  } catch (err) {
    // Re-throw UsernameLookupError instances as-is
    if (err && typeof err === "object" && "code" in err) {
      throw err;
    }

    // Handle network errors and other unexpected failures
    const error = new Error(
      err instanceof Error ? err.message : "Unknown error occurred during username lookup"
    ) as UsernameLookupError;
    error.code = "NETWORK_ERROR";
    throw error;
  }
}

/**
 * Safely resolves a username with automatic error handling and user-friendly messages.
 * Returns null on any error instead of throwing, making it suitable for UI flows
 * where you want to handle errors gracefully.
 * 
 * @param username - The username to resolve
 * @returns Promise resolving to receiver details or null on any error
 */
export async function safeResolveUsername(username: string): Promise<ReceiverDetails | null> {
  try {
    return await resolveUsername(username);
  } catch (error) {
    console.warn("Username resolution failed:", error);
    return null;
  }
}

/**
 * Batch resolve multiple usernames efficiently.
 * Note: Currently calls the single endpoint multiple times. Could be optimized
 * with a dedicated batch endpoint in the future.
 * 
 * @param usernames - Array of usernames to resolve
 * @returns Promise resolving to map of username -> ReceiverDetails (successful lookups only)
 */
export async function batchResolveUsernames(
  usernames: string[]
): Promise<Record<string, ReceiverDetails>> {
  if (!Array.isArray(usernames) || usernames.length === 0) {
    return {};
  }

  const results: Record<string, ReceiverDetails> = {};
  
  // Use Promise.allSettled to handle partial failures gracefully
  const promises = usernames.map(async (username) => {
    try {
      const result = await resolveUsername(username);
      return { username: username.trim().toLowerCase(), result };
    } catch {
      return { username: username.trim().toLowerCase(), result: null };
    }
  });

  const settled = await Promise.allSettled(promises);
  
  settled.forEach((outcome) => {
    if (outcome.status === "fulfilled" && outcome.value.result) {
      results[outcome.value.username] = outcome.value.result;
    }
  });

  return results;
}

/**
 * Resolves a username and returns the result in ZapsUser format for compatibility
 * with existing UI components that expect the full user interface.
 * 
 * @param username - The username to resolve
 * @returns Promise resolving to a partial ZapsUser with username and address
 * @throws UsernameLookupError with specific error codes for different failure scenarios
 */
export async function resolveUsernameAsZapsUser(username: string): Promise<{
  username: string;
  address: string;
  avatar_url: null;
  isVerified?: boolean;
}> {
  const receiverDetails = await resolveUsername(username);
  return {
    username: receiverDetails.username,
    address: receiverDetails.address,
    avatar_url: null, // resolve endpoint doesn't include avatar
    isVerified: false, // placeholder as noted in ZapsUser interface
  };
}
