"use client";

/**
 * Client-side context provider tree.
 *
 * Keeping all context providers in a single "use client" boundary lets
 * layout.tsx stay a Server Component (needed for `export const metadata`
 * and any future server-only data fetching), while every descendant
 * client component can call `usePrivy()`, `useAuth()`, or `useWallet()`
 * as needed.
 *
 * Provider order (outermost → innermost):
 *  PrivyProvider     – Privy SDK; must be the outermost so all Privy hooks
 *                      resolve correctly throughout the tree.
 *  AuthProvider      – Custom JWT / role session built on top of Privy.
 *  WalletProvider    – Freighter wallet session, restored on mount.
 *
 * Note: the theme provider (next-themes) is mounted in app/layout.tsx so the
 * `dark` class is toggled on the root <html> element as early as possible.
 */

import { PrivyProvider } from "@privy-io/react-auth";
import { AuthProvider } from "@/lib/auth-context";
import { WalletProvider } from "@/lib/wallet-context";
import type { ReactNode } from "react";

interface ProvidersProps {
  children: ReactNode;
}

export function Providers({ children }: ProvidersProps) {
  return (
    <PrivyProvider
      appId={process.env.NEXT_PUBLIC_PRIVY_APP_ID ?? ""}
      config={{
        loginMethods: ["google", "apple", "email"],
        appearance: { theme: "light" },
      }}
    >
      <AuthProvider>
        {/* #778 — single Freighter session for the whole app, restored on mount */}
        <WalletProvider>{children}</WalletProvider>
      </AuthProvider>
    </PrivyProvider>
  );
}
