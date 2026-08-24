import type { Metadata } from "next";
import "./globals.css";
import { PrivyProvider } from "@privy-io/react-auth";
import { ThemeProvider } from "next-themes";

export const metadata: Metadata = {
  title: "Zaps Merchant Dashboard",
  description: "Manage transactions, payouts, and analytics",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body>
        <PrivyProvider
          appId={process.env.NEXT_PUBLIC_PRIVY_APP_ID || ""}
          config={{
            loginMethods: ["google", "apple", "email"],
            appearance: { theme: "light" },
          }}
        >
          <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
            {children}
          </ThemeProvider>
        </PrivyProvider>
      </body>
    </html>
  );
}
