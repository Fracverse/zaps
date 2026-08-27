"use client";
import { createContext, useContext, useEffect, useState, ReactNode } from "react";
import { api, clearSession, storeSession } from "@/lib/api";

// ── Role constants ────────────────────────────────────────────────────────────

/** The role value returned by the backend login endpoint for super-admins. */
export const ROLE_SUPERADMIN = "superadmin";

// ── Context shape ─────────────────────────────────────────────────────────────

interface AuthCtx {
  token: string | null;
  /** Role string from the backend login response, e.g. "superadmin" | "admin" | "user". */
  role: string | null;
  login: (user_id: string, pin: string) => Promise<void>;
  logout: () => void;
}

const Ctx = createContext<AuthCtx>({
  token: null,
  role: null,
  login: async () => {},
  logout: () => {},
});

// ── Helpers ───────────────────────────────────────────────────────────────────

const ROLE_KEY = "role";

function readStoredRole(): string | null {
  return typeof window !== "undefined" ? localStorage.getItem(ROLE_KEY) : null;
}

// ── Provider ──────────────────────────────────────────────────────────────────

export function AuthProvider({ children }: { children: ReactNode }) {
  const [token, setToken] = useState<string | null>(null);
  // #786 — persist role alongside the JWT so it survives page refreshes
  const [role, setRole] = useState<string | null>(null);

  useEffect(() => {
    setToken(localStorage.getItem("token"));
    setRole(readStoredRole());
  }, []);

  const login = async (user_id: string, pin: string) => {
    const data = await api.login(user_id, pin);
    // The refresh token was previously discarded, which left the API client
    // with nothing to spend when the access token expired mid-session.
    storeSession(data.token, data.refresh_token);
    setToken(data.token);
    // #786 — persist the role returned by the backend so components can gate
    // on it without an extra API call.
    if (data.role) {
      localStorage.setItem(ROLE_KEY, data.role);
      setRole(data.role);
    }
  };

  const logout = () => {
    clearSession();
    localStorage.removeItem(ROLE_KEY);
    setToken(null);
    setRole(null);
  };

  return <Ctx.Provider value={{ token, role, login, logout }}>{children}</Ctx.Provider>;
}

export const useAuth = () => useContext(Ctx);

// ── #786 Role-gating hook ─────────────────────────────────────────────────────

/**
 * Returns true when the currently authenticated user holds the "superadmin"
 * role as returned by the backend login endpoint.
 *
 * Usage:
 *   const isSuperAdmin = useSuperAdmin();
 *   {isSuperAdmin && <DangerButton />}
 *   // or: <button disabled={!isSuperAdmin}>…</button>
 */
export function useSuperAdmin(): boolean {
  const { role } = useAuth();
  return role === ROLE_SUPERADMIN;
}
