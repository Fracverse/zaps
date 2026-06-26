"use client";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useAuth } from "@/lib/auth-context";

const nav = [
  { href: "/dashboard", label: "Overview", icon: "⬛" },
  { href: "/dashboard/transactions", label: "Transactions", icon: "📋" },
  { href: "/dashboard/payouts", label: "Payouts", icon: "💸" },
  { href: "/dashboard/qr", label: "QR Codes", icon: "⬜" },
  { href: "/dashboard/analytics", label: "Analytics", icon: "📈" },
  { href: "/dashboard/contracts", label: "Contracts", icon: "🔗" },
  { href: "/dashboard/yield", label: "Yield Vault", icon: "🏦" },
];

export default function Sidebar() {
  const path = usePathname();
  const { logout } = useAuth();

  return (
    <aside className="fixed inset-y-0 left-0 w-60 bg-slate-900 text-white flex flex-col z-20">
      <div className="px-6 py-5 border-b border-slate-700">
        <span className="text-xl font-bold tracking-tight">⚡ Zaps</span>
        <p className="text-xs text-slate-400 mt-0.5">Merchant Dashboard</p>
      </div>
      <nav className="flex-1 px-3 py-4 space-y-1">
        {nav.map(({ href, label, icon }) => (
          <Link
            key={href}
            href={href}
            className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors ${
              path === href
                ? "bg-indigo-600 text-white"
                : "text-slate-300 hover:bg-slate-800 hover:text-white"
            }`}
          >
            <span>{icon}</span>
            {label}
          </Link>
        ))}
      </nav>
      <div className="px-3 py-4 border-t border-slate-700">
        <button
          onClick={logout}
          className="w-full text-left px-3 py-2 text-sm text-slate-400 hover:text-white rounded-lg hover:bg-slate-800 transition-colors"
        >
          🚪 Sign out
        </button>
      </div>
    </aside>
  );
}
