"use client";

import { useEffect, useRef, useState } from "react";
import { format } from "date-fns";
import { useNotifications } from "@/lib/notifications-context";

const severityStyles: Record<string, string> = {
  info: "bg-blue-50 text-blue-700 ring-blue-600/20",
  warning: "bg-amber-50 text-amber-700 ring-amber-600/20",
  critical: "bg-red-50 text-red-700 ring-red-600/20",
};

const severityDot: Record<string, string> = {
  info: "bg-blue-500",
  warning: "bg-amber-500",
  critical: "bg-red-500",
};

export default function NotificationBell() {
  const { notifications, unreadCount, markAllRead, markRead } =
    useNotifications();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Close on outside click
  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, []);

  return (
    <div className="relative" ref={ref}>
      {/* Bell button */}
      <button
        aria-label={`Notifications${unreadCount > 0 ? ` — ${unreadCount} unread` : ""}`}
        onClick={() => setOpen((prev) => !prev)}
        className="relative flex h-9 w-9 items-center justify-center rounded-lg border border-slate-300 bg-white text-slate-600 hover:bg-slate-50 transition-colors focus:outline-none focus:ring-2 focus:ring-indigo-300"
      >
        {/* Bell icon */}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9" />
          <path d="M10.3 21a1.94 1.94 0 0 0 3.4 0" />
        </svg>

        {/* Unread badge */}
        {unreadCount > 0 && (
          <span
            aria-hidden="true"
            className="absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full bg-red-500 px-1 text-[10px] font-bold text-white"
          >
            {unreadCount > 99 ? "99+" : unreadCount}
          </span>
        )}
      </button>

      {/* Dropdown */}
      {open && (
        <div
          role="dialog"
          aria-label="Notifications"
          className="absolute right-0 top-11 z-50 w-80 rounded-xl border border-slate-200 bg-white shadow-xl"
        >
          {/* Header */}
          <div className="flex items-center justify-between border-b border-slate-100 px-4 py-3">
            <span className="text-sm font-semibold text-slate-900">
              Notifications
            </span>
            {unreadCount > 0 && (
              <button
                onClick={markAllRead}
                className="text-xs text-indigo-600 hover:underline focus:outline-none"
              >
                Mark all read
              </button>
            )}
          </div>

          {/* List */}
          <ul className="max-h-80 overflow-y-auto divide-y divide-slate-50">
            {notifications.length === 0 ? (
              <li className="px-4 py-8 text-center text-sm text-slate-400">
                No notifications yet
              </li>
            ) : (
              notifications.map((n) => (
                <li
                  key={n.id}
                  className={`px-4 py-3 ${!n.read ? "bg-slate-50" : ""}`}
                >
                  <button
                    className="flex w-full gap-3 text-left"
                    onClick={() => markRead(n.id)}
                  >
                    {/* Severity dot */}
                    <span
                      className={`mt-1.5 h-2 w-2 shrink-0 rounded-full ${severityDot[n.severity] ?? "bg-slate-400"}`}
                      aria-hidden="true"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-start justify-between gap-2">
                        <p className="text-sm font-medium text-slate-900 leading-snug">
                          {n.title}
                        </p>
                        <span
                          className={`mt-0.5 shrink-0 inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold ring-1 ring-inset ${severityStyles[n.severity] ?? ""}`}
                        >
                          {n.severity}
                        </span>
                      </div>
                      <p className="mt-0.5 text-xs text-slate-500 leading-snug">
                        {n.message}
                      </p>
                      <p className="mt-1 text-[10px] text-slate-400">
                        {format(new Date(n.timestamp), "MMM d, yyyy HH:mm")}
                      </p>
                    </div>
                    {!n.read && (
                      <span
                        className="mt-1.5 h-2 w-2 shrink-0 rounded-full bg-indigo-500"
                        aria-label="Unread"
                      />
                    )}
                  </button>
                </li>
              ))
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
