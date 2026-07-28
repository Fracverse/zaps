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
