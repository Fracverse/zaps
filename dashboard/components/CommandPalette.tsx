"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Command } from "cmdk";

interface NavItem {
  label: string;
  href: string;
  icon: string;
  keywords?: string;
}

const NAV_ITEMS: NavItem[] = [
  { label: "Overview", href: "/dashboard", icon: "⬛", keywords: "home summary stats" },
  { label: "Transactions", href: "/dashboard/transactions", icon: "📋", keywords: "payments social feed" },
  { label: "Payouts", href: "/dashboard/payouts", icon: "💸", keywords: "batch disburse withdraw" },
  { label: "QR Codes", href: "/dashboard/qr", icon: "⬜", keywords: "scan generate merchant" },
  { label: "Analytics", href: "/dashboard/analytics", icon: "📈", keywords: "charts reports graph" },
  { label: "Contracts", href: "/dashboard/contracts", icon: "🔗", keywords: "soroban smart contract health" },
  { label: "Yield Vault", href: "/dashboard/yield", icon: "🏦", keywords: "apy tvl deposit" },
];

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
}

export default function CommandPalette({ open, onClose }: CommandPaletteProps) {
  const router = useRouter();
  const [search, setSearch] = useState("");

  // Reset search whenever the palette opens
  useEffect(() => {
    if (open) setSearch("");
  }, [open]);

  // Close on Escape
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    if (open) document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  function navigate(href: string) {
    router.push(href);
    onClose();
  }

  return (
    /* Backdrop */
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-24 px-4"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      aria-modal="true"
      role="dialog"
      aria-label="Command palette"
    >
      <div className="w-full max-w-lg rounded-xl border border-slate-200 bg-white shadow-2xl overflow-hidden">
        <Command className="flex flex-col" shouldFilter>
          {/* Search input */}
          <div className="flex items-center gap-2 border-b border-slate-200 px-4 py-3">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              className="shrink-0 text-slate-400"
              aria-hidden="true"
            >
              <circle cx="11" cy="11" r="8" />
              <path d="m21 21-4.3-4.3" />
            </svg>
            <Command.Input
              value={search}
              onValueChange={setSearch}
              placeholder="Navigate to…"
              className="flex-1 bg-transparent text-sm text-slate-900 placeholder:text-slate-400 focus:outline-none"
              autoFocus
              aria-label="Search dashboard sections"
            />
            <kbd className="hidden sm:inline-flex h-5 items-center gap-1 rounded border border-slate-200 bg-slate-100 px-1.5 font-mono text-[10px] text-slate-500">
              ESC
            </kbd>
          </div>

          {/* Results */}
          <Command.List className="max-h-72 overflow-y-auto py-2">
            <Command.Empty className="py-6 text-center text-sm text-slate-400">
              No pages found.
            </Command.Empty>

            <Command.Group heading="Pages" className="px-2">
              {NAV_ITEMS.map((item) => (
                <Command.Item
                  key={item.href}
                  value={`${item.label} ${item.keywords ?? ""}`}
                  onSelect={() => navigate(item.href)}
                  className="flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2.5 text-sm text-slate-700
                             aria-selected:bg-indigo-50 aria-selected:text-indigo-700"
                >
                  <span aria-hidden="true">{item.icon}</span>
                  {item.label}
                </Command.Item>
              ))}
            </Command.Group>
          </Command.List>

          {/* Footer hint */}
          <div className="border-t border-slate-100 px-4 py-2 flex items-center gap-3 text-[10px] text-slate-400">
            <span>
              <kbd className="rounded border border-slate-200 bg-slate-100 px-1">↑</kbd>{" "}
              <kbd className="rounded border border-slate-200 bg-slate-100 px-1">↓</kbd>{" "}
              navigate
            </span>
            <span>
              <kbd className="rounded border border-slate-200 bg-slate-100 px-1">↵</kbd>{" "}
              select
            </span>
            <span>
              <kbd className="rounded border border-slate-200 bg-slate-100 px-1">Esc</kbd>{" "}
              close
            </span>
          </div>
        </Command>
      </div>
    </div>
  );
}
