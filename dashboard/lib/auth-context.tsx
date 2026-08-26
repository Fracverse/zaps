"use client";
import { createContext, useContext, useEffect, useState, ReactNode } from "react";
import { api, clearSession, storeSession } from "@/lib/api";

interface AuthCtx {
  token: string | null;
  login: (email: string, password: string) => Promise<void>;
  logout: () => void;
}

const Ctx = createContext<AuthCtx>({ token: null, login: async () => {}, logout: () => {} });

export function AuthProvider({ children }: { children: ReactNode }) {
  const [token, setToken] = useState<string | null>(null);

  useEffect(() => {
    setToken(localStorage.getItem("token"));
  }, []);

  const login = async (user_id: string, pin: string) => {
    const data = await api.login(user_id, pin);
    // The refresh token was previously discarded, which left the API client
    // with nothing to spend when the access token expired mid-session.
    storeSession(data.token, data.refresh_token);
    setToken(data.token);
  };

  const logout = () => {
    clearSession();
    setToken(null);
  };

  return <Ctx.Provider value={{ token, login, logout }}>{children}</Ctx.Provider>;
}

export const useAuth = () => useContext(Ctx);
