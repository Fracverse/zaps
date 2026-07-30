"use client";
import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { usePrivy } from "@privy-io/react-auth";
import Sidebar from "@/components/Sidebar";
import SearchBar from "@/components/SearchBar";

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
      <div className="flex min-h-screen items-center justify-center bg-slate-50">
        <div className="bg-white border border-slate-200 rounded-2xl shadow-sm p-8 w-full max-w-sm text-center">
          <span className="text-3xl">⚡</span>
          <h1 className="text-xl font-bold text-slate-900 mt-2">Zaps Merchant</h1>
          <p className="text-sm text-slate-500 mt-2">Sign in with Privy to access the dashboard</p>
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
    <div className="flex min-h-screen">
      <Sidebar />
      <main className="ml-60 flex-1 p-6 overflow-auto">
        <div className="mb-6">
          <SearchBar />
        </div>
        {children}
      </main>
    </div>
  );
}
