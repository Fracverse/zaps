/**
 * Dashboard navigation items.
 *
 * Shared by the desktop sidebar and the mobile drawer (#808) so the two cannot
 * drift into showing different routes.
 */
export interface DashboardNavItem {
  href: string;
  label: string;
  icon: string;
}

export const DASHBOARD_NAV: DashboardNavItem[] = [
  { href: "/dashboard", label: "Overview", icon: "⬛" },
  { href: "/dashboard/transactions", label: "Transactions", icon: "📋" },
  { href: "/dashboard/payouts", label: "Payouts", icon: "💸" },
  { href: "/dashboard/qr", label: "QR Codes", icon: "⬜" },
  { href: "/dashboard/analytics", label: "Analytics", icon: "📈" },
  { href: "/dashboard/contracts", label: "Contracts", icon: "🔗" },
  { href: "/dashboard/yield", label: "Yield Vault", icon: "🏦" },
];
