"use client";

/**
 * Freighter wallet session provider (#778).
 *
 * Wallet state used to live wherever a component happened to call the helpers
 * in `lib/freighter.ts`, so it was rebuilt per component and lost entirely on a
 * page refresh. This holds it in one React context and restores it on mount.
 *
 * Reconnect is deliberate rather than automatic: the provider only re-reads the
 * wallet when a previous session recorded that the user granted access (see
 * `WALLET_SESSION_KEY`). A first-time visitor is left disconnected and is never
 * shown an unprompted extension dialog.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  DEFAULT_WALLET_STATE,
  connectFreighter,
  detectFreighter,
  hasWalletSession,
  setWalletSession,
  signWithFreighter,
  type FreighterWalletState,
  type SignResult,
} from "@/lib/freighter";

export interface WalletContextValue extends FreighterWalletState {
  /** True while the on-mount reconnect is still resolving. */
  restoring: boolean;
  /** True while an explicit connect request is in flight. */
  connecting: boolean;
  /** Last connect or signing failure, cleared on the next attempt. */
  error: string | null;
  /** Prompt Freighter for access and persist the resulting session. */
  connect: () => Promise<void>;
  /** Forget the session locally. Freighter has no revoke API to call. */
  disconnect: () => void;
  /** Sign an XDR envelope with the connected wallet. */
  signTransaction: (
    xdr: string,
    networkPassphrase: string,
  ) => Promise<SignResult>;
}

const WalletContext = createContext<WalletContextValue>({
  ...DEFAULT_WALLET_STATE,
  restoring: false,
  connecting: false,
  error: null,
  connect: async () => {},
  disconnect: () => {},
  signTransaction: async () => {
    throw new Error("Wallet provider is not mounted");
  },
});

export function WalletProvider({ children }: { children: ReactNode }) {
  const [wallet, setWallet] = useState<FreighterWalletState>(
    DEFAULT_WALLET_STATE,
  );
  const [restoring, setRestoring] = useState(true);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Guards a late-resolving detect from writing state after unmount.
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  /**
   * Restore the wallet on mount.
   *
   * Runs in an effect rather than during render: `localStorage` and the
   * extension bridge are browser-only, so reading them while rendering would
   * make the first client render disagree with the server HTML.
   */
  useEffect(() => {
    let cancelled = false;

    async function restore() {
      if (!hasWalletSession()) {
        if (!cancelled) setRestoring(false);
        return;
      }

      try {
        const detected = await detectFreighter();
        if (cancelled || !mountedRef.current) return;

        setWallet(detected);
        // The extension may have been locked, uninstalled or disconnected
        // between visits; drop the stale flag rather than retrying forever.
        if (!detected.connected) setWalletSession(false);
      } catch {
        if (!cancelled) setWallet(DEFAULT_WALLET_STATE);
      } finally {
        if (!cancelled && mountedRef.current) setRestoring(false);
      }
    }

    void restore();
    return () => {
      cancelled = true;
    };
  }, []);

  const connect = useCallback(async () => {
    setConnecting(true);
    setError(null);
    try {
      const next = await connectFreighter();
      if (!mountedRef.current) return;
      setWallet(next);
      setWalletSession(next.connected);
    } catch (err) {
      if (!mountedRef.current) return;
      setWallet(DEFAULT_WALLET_STATE);
      setWalletSession(false);
      setError(err instanceof Error ? err.message : "Failed to connect wallet");
    } finally {
      if (mountedRef.current) setConnecting(false);
    }
  }, []);

  const disconnect = useCallback(() => {
    setWallet(DEFAULT_WALLET_STATE);
    setWalletSession(false);
    setError(null);
  }, []);

  const signTransaction = useCallback(
    async (xdr: string, networkPassphrase: string) => {
      if (!wallet.connected) {
        throw new Error("Connect a wallet before signing");
      }
      setError(null);
      try {
        return await signWithFreighter(xdr, networkPassphrase);
      } catch (err) {
        const message =
          err instanceof Error ? err.message : "Failed to sign transaction";
        if (mountedRef.current) setError(message);
        throw err instanceof Error ? err : new Error(message);
      }
    },
    [wallet.connected],
  );

  const value = useMemo<WalletContextValue>(
    () => ({
      ...wallet,
      restoring,
      connecting,
      error,
      connect,
      disconnect,
      signTransaction,
    }),
    [wallet, restoring, connecting, error, connect, disconnect, signTransaction],
  );

  return (
    <WalletContext.Provider value={value}>{children}</WalletContext.Provider>
  );
}

/** Read the shared Freighter wallet session. */
export const useWallet = () => useContext(WalletContext);
