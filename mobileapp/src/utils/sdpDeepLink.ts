/**
 * Deep link parser for Stellar Disbursement Platform (SDP) claiming invites.
 *
 * SDP sends recipients an invite URL (SMS/email) that must resolve into the
 * app so the recipient can validate their claim token and register a wallet.
 * We support both a custom-scheme link (native cold/warm start) and a plain
 * https link (in case the invite is opened from a browser or messaging app
 * that resolves universal links to the same route), e.g.:
 *
 *   zaps://claim?token=<SDP_TOKEN>
 *   zaps://claim/<SDP_TOKEN>
 *   https://zaps.app/claim?token=<SDP_TOKEN>
 *
 * Accepted token query params: `token`, `sdp_token`, `claim_token`.
 */

import * as Linking from "expo-linking";

export interface SdpClaimParseSuccess {
  valid: true;
  token: string;
}

export interface SdpClaimParseError {
  valid: false;
  error: string;
}

export type SdpClaimParseResult = SdpClaimParseSuccess | SdpClaimParseError;

const CLAIM_PATH_SEGMENT = "claim";
const TOKEN_PARAM_NAMES = ["token", "sdp_token", "claim_token"] as const;

// Conservative allow-list for token characters. SDP tokens are typically
// short-lived opaque identifiers (alphanumeric, dashes, underscores, dots).
// Anything outside this is rejected before we ever navigate with it.
const TOKEN_FORMAT_REGEX = /^[A-Za-z0-9._-]{6,256}$/;

export function isValidSdpToken(token: string): boolean {
  return typeof token === "string" && TOKEN_FORMAT_REGEX.test(token);
}

/**
 * Parse an incoming deep link URL and extract an SDP claim token, if present.
 * Returns a typed result so callers can distinguish "not a claim link"
 * (ignore) from "a claim link with a malformed token" (surface an error).
 */
export function parseSdpClaimUrl(url: string | null | undefined): SdpClaimParseResult {
  if (!url || typeof url !== "string") {
    return { valid: false, error: "Empty or invalid URL" };
  }

  let parsed: Linking.ParsedURL;
  try {
    parsed = Linking.parse(url);
  } catch {
    return { valid: false, error: "Could not parse URL" };
  }

  const pathSegments = (parsed.path ?? "")
    .split("/")
    .map((segment) => segment.trim())
    .filter(Boolean);

  const hostname = (parsed.hostname ?? "").toLowerCase();
  const isClaimLink =
    hostname === CLAIM_PATH_SEGMENT ||
    pathSegments[0]?.toLowerCase() === CLAIM_PATH_SEGMENT;

  if (!isClaimLink) {
    return { valid: false, error: "Not an SDP claim link" };
  }

  // Support `.../claim/<token>` as well as `.../claim?token=<token>`.
  const queryParams = parsed.queryParams ?? {};
  let token: string | undefined;

  for (const name of TOKEN_PARAM_NAMES) {
    const value = queryParams[name];
    if (typeof value === "string" && value.length > 0) {
      token = value;
      break;
    }
    if (Array.isArray(value) && typeof value[0] === "string") {
      token = value[0];
      break;
    }
  }

  if (!token) {
    // Fall back to a path-based token, e.g. zaps://claim/<token>
    // Custom-scheme URLs consume "claim" as the hostname, leaving the token
    // as the first (and only) path segment; https-style URLs keep "claim"
    // as the first path segment, so the token is the one after it.
    token =
      hostname === CLAIM_PATH_SEGMENT
        ? pathSegments[0]
        : pathSegments[0]?.toLowerCase() === CLAIM_PATH_SEGMENT
          ? pathSegments[1]
          : undefined;
  }

  if (!token) {
    return { valid: false, error: "Missing claim token in URL" };
  }

  const decoded = decodeURIComponent(token);

  if (!isValidSdpToken(decoded)) {
    return { valid: false, error: "Malformed claim token" };
  }

  return { valid: true, token: decoded };
}
