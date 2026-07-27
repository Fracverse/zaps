import AsyncStorage from "@react-native-async-storage/async-storage";
import { Buffer } from "buffer";

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";
const TOKEN_KEY = "auth_token";
const BASE58_ALPHABET =
  "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

async function getAuthHeaders(): Promise<Record<string, string>> {
  const token = await AsyncStorage.getItem(TOKEN_KEY);
  return {
    "Content-Type": "application/json",
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  };
}

interface ApiErrorResponse {
  error?: string;
  message?: string;
}

async function requestJson<T>(
  path: string,
  init: RequestInit = {}
): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...init.headers,
    },
  });

  if (!res.ok) {
    let message = `HTTP ${res.status}`;
    try {
      const body = (await res.json()) as ApiErrorResponse;
      message = body.error ?? body.message ?? message;
    } catch {
      // Keep the status-based fallback when the response has no JSON body.
    }
    throw new Error(message);
  }

  return res.json() as Promise<T>;
}

export interface PrivySigningProvider {
  request: (request: {
    method: "signMessage";
    params: { message: string };
  }) => Promise<{ signature: string | Uint8Array } | string | Uint8Array>;
  publicKey?: string;
  address?: string;
  _publicKey?: string;
}

type PrivyProviderResult =
  | PrivySigningProvider
  | { provider?: PrivySigningProvider | null }
  | null;

export interface PrivyEmbeddedWalletConnection {
  publicKey?: string;
  address?: string;
  provider?: PrivySigningProvider | null;
  wallets?: PrivyEmbeddedWalletConnection[];
  getProvider?: () => Promise<PrivySigningProvider>;
  create?: () => Promise<PrivyProviderResult>;
}

export interface PrivySignupInput {
  privyToken: string;
  privyDid: string;
  wallet: PrivyEmbeddedWalletConnection | PrivySigningProvider;
}

export interface AuthChallengeResponse {
  challenge: string;
}

export interface WalletAuthResponse {
  token: string;
  username: string | null;
}

export interface PrivyAuthResponse {
  token: string;
  username: string;
  privyDid: string;
}

export interface PrivySignupResult extends PrivyAuthResponse {
  stellarAddress: string;
}

function isSigningProvider(
  value: PrivyProviderResult | PrivyEmbeddedWalletConnection
): value is PrivySigningProvider {
  return (
    value !== null &&
    typeof value === "object" &&
    typeof (value as PrivySigningProvider).request === "function"
  );
}

function unwrapProvider(
  value: PrivyProviderResult
): PrivySigningProvider | null {
  if (isSigningProvider(value)) return value;
  return value?.provider && isSigningProvider(value.provider)
    ? value.provider
    : null;
}

function decodeBase58(value: string): Uint8Array {
  if (!value) throw new Error("Privy wallet public key is missing");

  const bytes = [0];
  for (const character of value) {
    const digit = BASE58_ALPHABET.indexOf(character);
    if (digit < 0) {
      throw new Error("Privy wallet returned an invalid public key");
    }

    let carry = digit;
    for (let index = 0; index < bytes.length; index++) {
      carry += bytes[index] * 58;
      bytes[index] = carry & 0xff;
      carry >>= 8;
    }

    while (carry > 0) {
      bytes.push(carry & 0xff);
      carry >>= 8;
    }
  }

  for (
    let index = 0;
    index < value.length - 1 && value[index] === "1";
    index++
  ) {
    bytes.push(0);
  }

  return Uint8Array.from(bytes.reverse());
}

export function stellarAddressFromPrivyPublicKey(publicKey: string): string {
  // Load the SDK only when the Privy signup path needs StrKey conversion.
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const stellarSdk: typeof import("@stellar/stellar-sdk") = require("@stellar/stellar-sdk");
  const { StrKey } = stellarSdk;
  const normalized = publicKey.trim();
  if (StrKey.isValidEd25519PublicKey(normalized)) return normalized;

  const rawPublicKey = decodeBase58(normalized);
  if (rawPublicKey.length !== 32) {
    throw new Error("Privy wallet public key must contain 32 bytes");
  }

  return StrKey.encodeEd25519PublicKey(Buffer.from(rawPublicKey));
}

export async function resolvePrivyWallet(
  connection: PrivyEmbeddedWalletConnection | PrivySigningProvider
): Promise<{ provider: PrivySigningProvider; stellarAddress: string }> {
  const wallet =
    !isSigningProvider(connection) && connection.wallets?.length
      ? connection.wallets[0]
      : connection;

  let provider =
    (!isSigningProvider(wallet) && wallet.provider) ||
    (!isSigningProvider(connection) && connection.provider) ||
    (isSigningProvider(wallet) ? wallet : null);

  if (!provider && !isSigningProvider(wallet) && wallet.getProvider) {
    provider = await wallet.getProvider();
  }
  if (
    !provider &&
    wallet !== connection &&
    !isSigningProvider(connection) &&
    connection.getProvider
  ) {
    provider = await connection.getProvider();
  }
  if (!provider && !isSigningProvider(connection) && connection.create) {
    provider = unwrapProvider(await connection.create());
  }
  if (!provider) {
    throw new Error("Privy embedded wallet provider is unavailable");
  }

  const publicKey =
    (!isSigningProvider(wallet) && (wallet.publicKey ?? wallet.address)) ||
    (!isSigningProvider(connection) &&
      (connection.publicKey ?? connection.address)) ||
    provider.publicKey ||
    provider.address ||
    provider._publicKey;

  if (!publicKey) {
    throw new Error("Privy embedded wallet public key is unavailable");
  }

  return {
    provider,
    stellarAddress: stellarAddressFromPrivyPublicKey(publicKey),
  };
}

export function getAuthChallenge(): Promise<AuthChallengeResponse> {
  return requestJson<AuthChallengeResponse>("/api/auth/challenge");
}

export function verifyWalletChallenge(input: {
  address: string;
  signature: string;
  challenge: string;
}): Promise<WalletAuthResponse> {
  return requestJson<WalletAuthResponse>("/api/auth/verify", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function linkPrivyAddress(input: {
  privyToken: string;
  privyDid: string;
  stellarAddress: string;
}): Promise<PrivyAuthResponse> {
  const response = await requestJson<{
    token: string;
    username: string;
    privy_did: string;
  }>("/api/auth/privy", {
    method: "POST",
    body: JSON.stringify({
      privy_token: input.privyToken,
      privy_did: input.privyDid,
      stellar_address: input.stellarAddress,
    }),
  });

  await AsyncStorage.setItem(TOKEN_KEY, response.token);

  return {
    token: response.token,
    username: response.username,
    privyDid: response.privy_did,
  };
}

export async function completePrivySignup({
  privyToken,
  privyDid,
  wallet,
}: PrivySignupInput): Promise<PrivySignupResult> {
  if (!privyToken.trim()) throw new Error("Privy access token is required");
  if (!privyDid.trim()) throw new Error("Privy user DID is required");

  const { provider, stellarAddress } = await resolvePrivyWallet(wallet);
  const { challenge } = await getAuthChallenge();
  if (!challenge) throw new Error("Authentication challenge is missing");

  const signed = await provider.request({
    method: "signMessage",
    params: {
      message: Buffer.from(challenge, "utf8").toString("base64"),
    },
  });
  const signatureValue =
    typeof signed === "object" && "signature" in signed
      ? signed.signature
      : signed;
  const signature =
    typeof signatureValue === "string"
      ? signatureValue
      : Buffer.from(signatureValue).toString("base64");

  if (!signature) throw new Error("Privy wallet returned an empty signature");

  await verifyWalletChallenge({
    address: stellarAddress,
    signature,
    challenge,
  });
  const linked = await linkPrivyAddress({
    privyToken,
    privyDid,
    stellarAddress,
  });

  return { ...linked, stellarAddress };
}

export const signupWithPrivy = completePrivySignup;

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

export interface YieldBalance {
  apy: string | number;
  totalYieldEarned: string | number;
  availableBalance: string | number;
  earningBalance: string | number;
  explanation: string;
  autoEarnEnabled: boolean;
}

export async function fetchYieldBalance(): Promise<YieldBalance> {
  const headers = await getAuthHeaders();
  const res = await fetch(`${API_BASE}/api/yield/balance`, { headers });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json() as Promise<YieldBalance>;
}

export async function updateAutoEarn(enabled: boolean): Promise<void> {
  const headers = await getAuthHeaders();
  try {
    await fetch(`${API_BASE}/api/yield/auto-earn`, {
      method: "PATCH",
      headers,
      body: JSON.stringify({ enabled }),
    });
  } catch {
    // Non-fatal — local state already reflects the toggle
  }
}
