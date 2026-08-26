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

/** The subset of `@stellar/freighter-api` this module uses. */
export interface FreighterApi {
  isConnected: () => Promise<{ isConnected: boolean }>;
  getAddress: () => Promise<{ address?: string }>;
  getNetwork: () => Promise<{ network?: string }>;
  requestAccess: () => Promise<{ address?: string; error?: string }>;
  signTransaction: (
    xdr: string,
    opts: { networkPassphrase: string },
  ) => Promise<{ signedTxXdr?: string; error?: string }>;
}

/** Global under which a stand-in Freighter API can be injected. */
export const FREIGHTER_MOCK_KEY = "__zapsFreighterApiMock__";

/**
 * Resolve the Freighter API.
 *
 * Freighter is a browser extension, so end-to-end tests have no way to install
 * or drive it. When `window.__zapsFreighterApiMock__` is present the module
 * uses that instead of the real package, which is what lets the Playwright
 * suite exercise the signing flow. The hook is ignored in production builds,
 * so a page served to a user can never be talked into using a fake wallet by
 * anything that manages to set the global.
 */
export async function loadFreighterApi(): Promise<FreighterApi> {
  if (process.env.NODE_ENV !== "production" && typeof window !== "undefined") {
    const injected = (window as unknown as Record<string, unknown>)[
      FREIGHTER_MOCK_KEY
    ];
    if (injected) return injected as FreighterApi;
  }
  return (await import("@stellar/freighter-api")) as unknown as FreighterApi;
}

/**
 * Detect whether Freighter is installed in the browser and, if so,
 * whether it is already connected and which public key / network are active.
 */
export async function detectFreighter(): Promise<FreighterWalletState> {
  try {
    const { isConnected, getAddress, getNetwork } = await loadFreighterApi();

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
    const { requestAccess } = await loadFreighterApi();
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
  const { signTransaction } = await loadFreighterApi();
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
