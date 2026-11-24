/**
 * contacts.ts
 *
 * Stellar Contact Lookup Integration — Issue #699
 *
 * Provides functions to:
 *  1. Request permission and fetch the device contact list via Expo Contacts.
 *  2. Normalize and SHA-256 hash phone numbers locally before they leave the
 *     device (user privacy).
 *  3. Query the backend batch-match endpoint (`POST /api/v1/contacts/match`)
 *     with the hashed phone numbers.
 *  4. Return a list of `MatchedContact` items that marry the local contact
 *     display name with the returned Zaps username / Stellar address.
 *
 * Usage:
 *   import { fetchMatchedContacts } from './contacts';
 *
 *   const matches = await fetchMatchedContacts();
 *   // [{ contactName: "Tolu O.", username: "tolu.zaps", address: "G..." }, …]
 */

import * as Contacts from "expo-contacts";

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface MatchedContact {
  /** Display name from the device contact list. */
  contactName: string;
  /** Zaps username returned by the backend match. */
  username: string;
  /** Stellar address associated with the username. */
  address: string;
  /** The normalised phone number that was matched (for display). */
  phoneNumber: string;
  /** Optional avatar URL if provided by the backend. */
  avatarUrl?: string | null;
}

export interface ContactPermissionResult {
  granted: boolean;
  /** Populated when `granted === false`. */
  reason?: "denied" | "restricted" | "unavailable";
}

// Internal shape returned by the backend.
interface BackendContactMatch {
  phone_hash: string;
  username: string;
  address: string;
  avatar_url?: string | null;
}

// ── Phone normalisation ───────────────────────────────────────────────────────

/**
 * Strip all non-digit characters from a phone number, then remove a leading
 * `0` (common in Nigerian/international number formatting) to get a canonical
 * representation that can be hashed consistently.
 *
 * Examples:
 *   "+234 801 234 5678" → "2348012345678"
 *   "0801 234 5678"     → "8012345678"
 *   "+1 (555) 000-1234" → "15550001234"
 */
export function normalizePhoneNumber(phone: string): string {
  // Remove every character that is not a digit or leading +
  const digitsOnly = phone.replace(/\D/g, "");
  // Drop a single leading zero (common in local-format numbers)
  return digitsOnly.startsWith("0") ? digitsOnly.slice(1) : digitsOnly;
}

// ── SHA-256 hashing ───────────────────────────────────────────────────────────

/**
 * Compute the hex-encoded SHA-256 hash of a string using the Web Crypto API
 * (available on both React Native JSI via `react-native-quick-crypto` polyfill
 * and in the Jest test environment via jsdom).
 *
 * If the SubtleCrypto API is unavailable (e.g. very old RN environments), the
 * function falls back to a deterministic FNV-1a hex string so the code
 * doesn't hard-crash — production deployments should always have Web Crypto.
 */
export async function sha256Hex(input: string): Promise<string> {
  try {
    const encoder = new TextEncoder();
    const data = encoder.encode(input);
    const hashBuffer = await crypto.subtle.digest("SHA-256", data);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map((b) => b.toString(16).padStart(2, "0")).join("");
  } catch {
    // FNV-1a fallback (deterministic, NOT cryptographic — for non-prod envs).
    let h = 0x811c9dc5;
    for (let i = 0; i < input.length; i++) {
      h ^= input.charCodeAt(i);
      h = (h * 0x01000193) >>> 0;
    }
    return h.toString(16).padStart(8, "0");
  }
}

/**
 * Normalise a raw phone number string and return its SHA-256 hex hash.
 * Returns `null` when the normalised number is fewer than 7 digits (likely
 * garbage data) to avoid polluting the batch request.
 */
export async function hashPhoneNumber(
  rawPhone: string
): Promise<string | null> {
  const normalised = normalizePhoneNumber(rawPhone);
  if (normalised.length < 7) return null;
  return sha256Hex(normalised);
}

// ── Permissions ───────────────────────────────────────────────────────────────

/**
 * Request permission to access the device contact list.
 * Returns a typed result so callers can distinguish "denied" from "unavailable".
 */
export async function requestContactsPermission(): Promise<ContactPermissionResult> {
  try {
    const { status } = await Contacts.requestPermissionsAsync();

    if (status === "granted") {
      return { granted: true };
    }

    if (status === "denied") {
      return { granted: false, reason: "denied" };
    }

    // "restricted" or any unexpected status
    return { granted: false, reason: "restricted" };
  } catch {
    return { granted: false, reason: "unavailable" };
  }
}

// ── Contact fetching & hashing ────────────────────────────────────────────────

interface PhoneHashMap {
  /** Map from hex hash → { contactName, phoneNumber }. */
  [hash: string]: { contactName: string; phoneNumber: string };
}

/**
 * Fetch all contacts from the device, normalise & hash every phone number,
 * and return a map of `{ hash → { contactName, phoneNumber } }`.
 *
 * Only includes contacts that have at least one phone number. Contacts with
 * multiple numbers contribute one entry per valid hash.
 */
export async function buildPhoneHashMap(): Promise<PhoneHashMap> {
  const { data } = await Contacts.getContactsAsync({
    fields: [Contacts.Fields.PhoneNumbers, Contacts.Fields.Name],
  });

  const map: PhoneHashMap = {};

  await Promise.all(
    data.map(async (contact) => {
      if (!contact.phoneNumbers || contact.phoneNumbers.length === 0) return;

      const displayName =
        contact.name ||
        `${contact.firstName ?? ""} ${contact.lastName ?? ""}`.trim() ||
        "Unknown";

      for (const phoneEntry of contact.phoneNumbers) {
        const raw = phoneEntry.number ?? "";
        if (!raw) continue;

        const hash = await hashPhoneNumber(raw);
        if (!hash) continue;

        // First match wins if the same hash appears multiple times.
        if (!map[hash]) {
          map[hash] = {
            contactName: displayName,
            phoneNumber: normalizePhoneNumber(raw),
          };
        }
      }
    })
  );

  return map;
}

// ── Backend batch match ───────────────────────────────────────────────────────

/**
 * POST the array of hashed phone numbers to the backend and retrieve the
 * matching Zaps usernames / Stellar addresses.
 *
 * Endpoint: `POST /api/v1/contacts/match`
 * Request:  `{ phone_hashes: string[] }`
 * Response: `{ matches: BackendContactMatch[] }`
 */
export async function batchMatchContacts(
  phoneHashes: string[],
  authToken?: string
): Promise<BackendContactMatch[]> {
  if (phoneHashes.length === 0) return [];

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(authToken ? { Authorization: `Bearer ${authToken}` } : {}),
  };

  const response = await fetch(`${API_BASE}/api/v1/contacts/match`, {
    method: "POST",
    headers,
    body: JSON.stringify({ phone_hashes: phoneHashes }),
  });

  if (!response.ok) {
    throw new Error(`Contact match endpoint returned HTTP ${response.status}`);
  }

  const body = (await response.json()) as {
    matches?: BackendContactMatch[];
  };

  return body.matches ?? [];
}

// ── Public API ────────────────────────────────────────────────────────────────

export interface FetchMatchedContactsOptions {
  /**
   * Optional auth token.  When omitted the function will attempt an
   * unauthenticated request (the backend may accept or reject it depending on
   * configuration).
   */
  authToken?: string;
  /**
   * Maximum number of hashes to include in a single batch request.
   * Defaults to 500 to stay within typical API payload limits.
   */
  batchSize?: number;
}

/**
 * Full pipeline: request permissions → fetch contacts → hash phones →
 * batch-match against backend → return `MatchedContact[]`.
 *
 * Returns an empty array (never throws) when:
 *  - The user denies contact permission.
 *  - The device has no contacts.
 *  - The backend returns no matches.
 *
 * @throws when a network error occurs during the batch-match request so the
 * caller can surface a meaningful error message.
 */
export async function fetchMatchedContacts(
  options: FetchMatchedContactsOptions = {}
): Promise<MatchedContact[]> {
  const { authToken, batchSize = 500 } = options;

  // Step 1: Request permission
  const permission = await requestContactsPermission();
  if (!permission.granted) {
    return [];
  }

  // Step 2: Build phone → hash map
  const hashMap = await buildPhoneHashMap();
  const allHashes = Object.keys(hashMap);

  if (allHashes.length === 0) {
    return [];
  }

  // Step 3: Batch match (split into chunks to avoid oversized payloads)
  const allMatches: BackendContactMatch[] = [];

  for (let i = 0; i < allHashes.length; i += batchSize) {
    const chunk = allHashes.slice(i, i + batchSize);
    const chunkMatches = await batchMatchContacts(chunk, authToken);
    allMatches.push(...chunkMatches);
  }

  // Step 4: Merge backend results with local contact metadata
  return allMatches.reduce<MatchedContact[]>((acc, match) => {
    const local = hashMap[match.phone_hash];
    if (!local) return acc; // shouldn't happen but guard anyway

    acc.push({
      contactName: local.contactName,
      username: match.username,
      address: match.address,
      phoneNumber: local.phoneNumber,
      avatarUrl: match.avatar_url ?? null,
    });

    return acc;
  }, []);
}
