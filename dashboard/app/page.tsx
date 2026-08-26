"use client";
import { useRouter } from "next/navigation";
import { usePrivy } from "@privy-io/react-auth";
import { useEffect } from "react";

export default function LoginPage() {
  const { authenticated, login, logout } = usePrivy();
  const router = useRouter();

  useEffect(() => {
    if (authenticated) {
      document.cookie = "zaps-auth=1; path=/; SameSite=Lax";
      router.replace("/dashboard");
    }
  }, [authenticated, router]);

  return (
    <div className="min-h-screen flex items-center justify-center bg-slate-50">
      <div className="bg-white border border-slate-200 rounded-2xl shadow-sm p-8 w-full max-w-sm">
        <div className="text-center mb-6">
          <span className="text-3xl">⚡</span>
          <h1 className="text-xl font-bold text-slate-900 mt-2">Zaps Merchant</h1>
          <p className="text-sm text-slate-500">Sign in with Privy to continue</p>
        </div>
        <button
          onClick={() => login()}
          className="w-full bg-indigo-600 text-white py-2.5 rounded-lg text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
        >
          Sign in with Privy
        </button>
        {authenticated && (
          <button
            onClick={() => { logout(); router.replace("/"); }}
            className="mt-3 w-full border border-slate-300 text-slate-700 py-2.5 rounded-lg text-sm font-medium hover:bg-slate-50 transition-colors"
          >
            Sign out
          </button>
        )}
      </div>
    </div>
  );
}
