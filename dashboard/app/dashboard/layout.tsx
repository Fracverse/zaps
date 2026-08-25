"use client";

import { useCallback, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { usePrivy } from "@privy-io/react-auth";
import Sidebar from "@/components/Sidebar";
import SearchBar from "@/components/SearchBar";
import ThemeSelector from "@/components/ThemeSelector";
import NotificationBell from "@/components/NotificationBell";
import CommandPalette from "@/components/CommandPalette";
import { NotificationsProvider } from "@/lib/notifications-context";

function DashboardShell({ children }: { children: React.ReactNode }) {
  const [paletteOpen, setPaletteOpen] = useState(false);

  // #793 — Cmd+K / Ctrl+K opens the command palette
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "k") {
      e.preventDefault();
      setPaletteOpen((prev) => !prev);
    }
  }, []);

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  return (
    <>
      <div className="flex min-h-screen bg-slate-50 dark:bg-slate-950">
        <Sidebar />
        <main className="ml-60 flex-1 p-6 overflow-auto">
          <div className="mb-6 flex items-center justify-between gap-4">
            {/* Search bar with Cmd+K hint */}
            <div className="flex-1 flex items-center gap-2">
              <div className="flex-1">
                <SearchBar />
              </div>
              {/* Cmd+K shortcut hint pill */}
              <button
                onClick={() => setPaletteOpen(true)}
                aria-label="Open command palette (Cmd+K)"
                className="hidden sm:inline-flex items-center gap-1.5 rounded-lg border border-slate-300 bg-white px-2.5 py-2 text-xs text-slate-500 hover:bg-slate-50 transition-colors whitespace-nowrap"
              >
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="13"
                  height="13"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  aria-hidden="true"
                >
                  <circle cx="11" cy="11" r="8" />
                  <path d="m21 21-4.3-4.3" />
                </svg>
                <kbd className="font-mono">⌘K</kbd>
              </button>
            </div>

            {/* Right-side controls */}
            <div className="flex items-center gap-2">
              {/* #798 — Notification bell */}
              <NotificationBell />
              <ThemeSelector />
            </div>
          </div>

          {children}
        </main>
      </div>

      {/* #793 — Command palette modal */}
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
    </>
  );
}

export default function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const { authenticated, login } = usePrivy();
  const router = useRouter();

  useEffect(() => {
    if (!authenticated) {
      router.replace("/");
    }
  }, [authenticated, router]);

  if (!authenticated) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-slate-50 dark:bg-slate-950">
        <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl shadow-sm p-8 w-full max-w-sm text-center">
          <span className="text-3xl">⚡</span>
          <h1 className="text-xl font-bold text-slate-900 dark:text-slate-50 mt-2">
            Zaps Merchant
          </h1>
          <p className="text-sm text-slate-500 dark:text-slate-400 mt-2">
            Sign in with Privy to access the dashboard
          </p>
          <button
            onClick={() => login()}
            className="mt-4 w-full bg-indigo-600 text-white py-2.5 rounded-lg text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
          >
            Sign in with Privy
          </button>
        </div>
      </div>
    );
  }

  return (
    // #798 — Wrap authenticated shell in NotificationsProvider
    <NotificationsProvider>
      <DashboardShell>{children}</DashboardShell>
    </NotificationsProvider>
  );
}
