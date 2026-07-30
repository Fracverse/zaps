import type { Metadata } from "next";
import "./globals.css";
import { PrivyProvider } from "@privy-io/react-auth";

export const metadata: Metadata = {
  title: "Zaps Merchant Dashboard",
  description: "Manage transactions, payouts, and analytics",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>
        <PrivyProvider
          appId={process.env.NEXT_PUBLIC_PRIVY_APP_ID || ""}
          config={{
            loginMethods: ["google", "apple", "email"],
            appearance: { theme: "light" },
          }}
        >
          {children}
        </PrivyProvider>
      </body>
    </html>
  );
}
