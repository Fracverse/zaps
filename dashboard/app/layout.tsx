import type { Metadata } from "next";
import "./globals.css";
import { Providers } from "./providers";

export const metadata: Metadata = {
  title: "Zaps Merchant Dashboard",
  description: "Manage transactions, payouts, and analytics",
};

/**
 * Root layout — Server Component.
 *
 * All client-side context providers live in <Providers> (providers.tsx),
 * which carries the "use client" boundary. This keeps the layout itself
 * server-renderable so `metadata` exports, future `fetch()` calls, and
 * React Server Component optimisations all work as expected.
 *
 * Auth state is accessible in any client component via:
 *   const { authenticated } = usePrivy();   // Privy session
 *   const { token, role }   = useAuth();    // backend JWT / role
 *   const isSuperAdmin      = useSuperAdmin();
 */
export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body>
        <Providers>{children}</Providers>
      </body>
    </html>
  );
}
