/**
 * freighter.ts
 * Helper utilities for @stellar/freighter-api browser wallet integration.
 * All functions are async-safe and handle the case where Freighter is not installed.
 */

export interface FreighterWalletState {
  installed: boolean;
  connected: boolean;
  publicKey: string | null;
  network: string | null;
}

/** Default disconnected state returned before any detection. */
export const DEFAULT_WALLET_STATE: FreighterWalletState = {
  installed: false,
  connected: false,
  publicKey: null,
  network: null,
};

/**
 * Detect whether Freighter is installed in the browser and, if so,
 * whether it is already connected and which public key / network are active.
 */
export async function detectFreighter(): Promise<FreighterWalletState> {
  try {
    const { isConnected, getAddress, getNetwork } = await import(
      "@stellar/freighter-api"
    );

    const connectedResult = await isConnected();
    if (!connectedResult.isConnected) {
      return { installed: true, connected: false, publicKey: null, network: null };
    }

    const [addrResult, networkResult] = await Promise.allSettled([
      getAddress(),
      getNetwork(),
    ]);

    const publicKey =
      addrResult.status === "fulfilled" && addrResult.value.address
        ? addrResult.value.address
        : null;

    const network =
      networkResult.status === "fulfilled" && networkResult.value.network
        ? networkResult.value.network
        : null;

    return { installed: true, connected: true, publicKey, network };
  } catch {
    // Extension not installed or not accessible (SSR / non-browser context).
    return DEFAULT_WALLET_STATE;
  }
}

/**
 * Request the user to connect / grant access to Freighter.
 * Returns the resulting wallet state after the prompt.
 */
export async function connectFreighter(): Promise<FreighterWalletState> {
  try {
    const { requestAccess } = await import("@stellar/freighter-api");
    const result = await requestAccess();
    if ("error" in result && result.error) {
      throw new Error(result.error);
    }
    return detectFreighter();
  } catch (err) {
    throw err instanceof Error ? err : new Error("Failed to connect Freighter");
  }
}

export interface SignResult {
  signedTxXdr: string;
}

/**
 * Sign an XDR-encoded transaction envelope with Freighter.
 *
 * @param xdr              Base64-encoded XDR transaction envelope.
 * @param networkPassphrase Stellar network passphrase (testnet or mainnet).
 * @returns                Signed XDR string.
 */
export async function signWithFreighter(
  xdr: string,
  networkPassphrase: string
): Promise<SignResult> {
  const { signTransaction } = await import("@stellar/freighter-api");
  const result = await signTransaction(xdr, { networkPassphrase });
  if ("error" in result && result.error) {
    throw new Error(result.error);
  }
  return result as SignResult;
}

/** Truncate a Stellar public key for display: "GABC…WXYZ". */
export function truncateKey(key: string, head = 4, tail = 4): string {
  if (key.length <= head + tail + 3) return key;
  return `${key.slice(0, head)}…${key.slice(-tail)}`;
}

// ── Wallet session persistence (#778) ────────────────────────────────────────

/**
 * localStorage key recording that the user has previously granted access.
 *
 * Freighter's `isConnected()` reports whether the extension is present and
 * unlocked, not whether *this site* was ever authorised. Without a local record
 * a page refresh would either silently show a wallet the user never connected,
 * or pop an access prompt on every load. This flag is what makes the reconnect
 * on mount deliberate rather than either of those.
 */
export const WALLET_SESSION_KEY = "zaps.wallet.connected";

/** Whether a previous session recorded a granted connection. */
export function hasWalletSession(): boolean {
  try {
    if (typeof window === "undefined") return false;
    return window.localStorage.getItem(WALLET_SESSION_KEY) === "true";
  } catch {
    // Private mode or a blocked store costs reconnect, not the app.
    return false;
  }
}

/** Record, or clear, the granted-connection flag for future page loads. */
export function setWalletSession(connected: boolean): void {
  try {
    if (typeof window === "undefined") return;
    if (connected) window.localStorage.setItem(WALLET_SESSION_KEY, "true");
    else window.localStorage.removeItem(WALLET_SESSION_KEY);
  } catch {
    /* best-effort */
  }
}
