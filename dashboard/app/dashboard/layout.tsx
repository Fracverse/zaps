"use client";
import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { usePrivy } from "@privy-io/react-auth";
import Sidebar from "@/components/Sidebar";
import SearchBar from "@/components/SearchBar";
import ThemeSelector from "@/components/ThemeSelector";

export default function DashboardLayout({ children }: { children: React.ReactNode }) {
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
          <h1 className="text-xl font-bold text-slate-900 dark:text-slate-50 mt-2">Zaps Merchant</h1>
          <p className="text-sm text-slate-500 dark:text-slate-400 mt-2">Sign in with Privy to access the dashboard</p>
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
    <div className="flex min-h-screen bg-slate-50 dark:bg-slate-950">
      <Sidebar />
      <main className="ml-60 flex-1 p-6 overflow-auto">
        <div className="mb-6 flex items-center justify-between gap-4">
          <div className="flex-1">
            <SearchBar />
          </div>
          <ThemeSelector />
        </div>
        {children}
      </main>
    </div>
  );
}
