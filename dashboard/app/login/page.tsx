"use client";

import { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import { usePrivy } from "@privy-io/react-auth";

/**
 * Admin Login Page (#1022)
 * Integrates Privy Web SDK authorization for admin profiles with PIN fallback.
 */
export default function LoginPage() {
  const { login: pinLogin } = useAuth();
  const { authenticated, login: privyLogin, ready } = usePrivy();
  const router = useRouter();
  const [userId, setUserId] = useState("");
  const [pin, setPin] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // Once authenticated via Privy Web SDK, set auth cookie and route to dashboard
  useEffect(() => {
    if (authenticated) {
      document.cookie = "zaps-auth=1; path=/; SameSite=Lax";
      router.replace("/dashboard");
    }
  }, [authenticated, router]);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      await pinLogin(userId, pin);
      router.replace("/dashboard");
    } catch {
      setError("Invalid user ID or PIN. Please try again.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-slate-50">
      <div className="bg-white border border-slate-200 rounded-2xl shadow-sm p-8 w-full max-w-sm">
        <div className="text-center mb-6">
          <span className="text-3xl">⚡</span>
          <h1 className="text-xl font-bold text-slate-900 mt-2">Zaps Admin</h1>
          <p className="text-sm text-slate-500">Sign in to manage disbursements & operations</p>
        </div>

        {error && (
          <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">{error}</div>
        )}

        {/* ── #1022: Privy Web SDK Authentication ── */}
        <div className="mb-5">
          <button
            type="button"
            onClick={() => privyLogin()}
            data-testid="privy-login-btn"
            className="w-full bg-indigo-600 text-white py-2.5 rounded-lg text-sm font-medium hover:bg-indigo-700 transition-colors flex items-center justify-center gap-2 shadow-xs"
          >
            <span>🔐</span>
            <span>Sign in with Privy</span>
          </button>
        </div>

        <div className="relative my-4 flex items-center justify-center">
          <div className="border-t border-slate-200 w-full" />
          <span className="bg-white px-2 text-xs uppercase tracking-wider text-slate-400 font-semibold absolute">
            or PIN auth
          </span>
        </div>

        <form onSubmit={submit} className="space-y-4">
          <div>
            <label className="block text-xs font-medium text-slate-600 mb-1">User ID</label>
            <input
              required
              type="text"
              placeholder="Your user ID"
              value={userId}
              onChange={(e) => setUserId(e.target.value)}
              className="w-full border border-slate-300 rounded-lg px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
          </div>
          <div>
            <label className="block text-xs font-medium text-slate-600 mb-1">PIN (4–6 digits)</label>
            <input
              required
              type="password"
              inputMode="numeric"
              pattern="[0-9]{4,6}"
              maxLength={6}
              placeholder="••••"
              value={pin}
              onChange={(e) => setPin(e.target.value)}
              className="w-full border border-slate-300 rounded-lg px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
          </div>
          <button
            type="submit"
            disabled={loading}
            className="w-full border border-slate-300 text-slate-700 py-2.5 rounded-lg text-sm font-medium hover:bg-slate-50 disabled:opacity-50 transition-colors"
          >
            {loading ? "Signing in…" : "Sign In with PIN"}
          </button>
        </form>
      </div>
    </div>
  );
}
