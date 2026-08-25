"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  ReactNode,
} from "react";
import { api, type ContractAlert } from "@/lib/api";

export interface AppNotification {
  id: string;
  title: string;
  message: string;
  severity: "info" | "warning" | "critical";
  timestamp: string;
  read: boolean;
}

interface NotificationsCtx {
  notifications: AppNotification[];
  unreadCount: number;
  markAllRead: () => void;
  markRead: (id: string) => void;
}

const Ctx = createContext<NotificationsCtx>({
  notifications: [],
  unreadCount: 0,
  markAllRead: () => {},
  markRead: () => {},
});

function alertToNotification(alert: ContractAlert): AppNotification {
  return {
    id: alert.id,
    title: alert.title,
    message: alert.message,
    severity: alert.severity,
    timestamp: alert.timestamp,
    read: false,
  };
}

export function NotificationsProvider({ children }: { children: ReactNode }) {
  const [notifications, setNotifications] = useState<AppNotification[]>([]);
  // Track ids already seen so we don't duplicate on re-poll
  const seenIds = useRef<Set<string>>(new Set());

  const fetchAlerts = useCallback(async () => {
    try {
      const { alerts } = await api.contractAlerts();
      setNotifications((prev) => {
        const incoming = alerts
          .filter((a) => !seenIds.current.has(a.id))
          .map(alertToNotification);

        if (incoming.length === 0) return prev;

        incoming.forEach((n) => seenIds.current.add(n.id));
        // New alerts go to the front; preserve existing read state
        return [...incoming, ...prev];
      });
    } catch {
      // Non-fatal — silently keep existing notifications
    }
  }, []);

  useEffect(() => {
    fetchAlerts();
    const interval = setInterval(fetchAlerts, 30_000);
    return () => clearInterval(interval);
  }, [fetchAlerts]);

  const markAllRead = useCallback(() => {
    setNotifications((prev) => prev.map((n) => ({ ...n, read: true })));
  }, []);

  const markRead = useCallback((id: string) => {
    setNotifications((prev) =>
      prev.map((n) => (n.id === id ? { ...n, read: true } : n)),
    );
  }, []);

  const unreadCount = notifications.filter((n) => !n.read).length;

  return (
    <Ctx.Provider value={{ notifications, unreadCount, markAllRead, markRead }}>
      {children}
    </Ctx.Provider>
  );
}

export const useNotifications = () => useContext(Ctx);
