import AsyncStorage from "@react-native-async-storage/async-storage";

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";
const TOKEN_KEY = "auth_token";

async function getAuthHeaders(): Promise<Record<string, string>> {
  const token = await AsyncStorage.getItem(TOKEN_KEY);
  return {
    "Content-Type": "application/json",
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  };
}

export type ClaimValidationStatus = "valid" | "expired" | "already_claimed" | "invalid";

export interface ClaimValidationResult {
  status: ClaimValidationStatus;
  recipientName?: string;
  amount?: string;
  assetCode?: string;
}

/**
 * Validate an SDP claim token against the backend.
 *
 * NOTE: the `/api/disbursements/claim/validate` endpoint is not live yet
 * (tracked separately on the backend). Until then this resolves to a mock
 * "valid" response so the mobile validation screen and navigation flow can
 * be built and tested end-to-end; swap this for a hard failure once the
 * endpoint ships.
 */
export async function validateClaimToken(
  token: string
): Promise<ClaimValidationResult> {
  const headers = await getAuthHeaders();
  try {
    const res = await fetch(
      `${API_BASE}/api/disbursements/claim/validate?token=${encodeURIComponent(token)}`,
      { headers }
    );
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return (await res.json()) as ClaimValidationResult;
  } catch {
    // Fallback to mock while the backend endpoint is not yet live.
    return {
      status: "valid",
      amount: "5,000.00",
      assetCode: "USDC",
    };
  }
}
