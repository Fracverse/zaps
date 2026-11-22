import type { Metadata } from "next";
import { ThemeProvider } from "next-themes";
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
        {/* #1040 — next-themes toggles the `dark` class on <html> and persists
            the choice in localStorage (`theme`), applied before first paint. */}
        <ThemeProvider
          attribute="class"
          defaultTheme="system"
          enableSystem
          enableColorScheme
          storageKey="theme"
        >
          <Providers>{children}</Providers>
        </ThemeProvider>
      </body>
    </html>
  );
}
