"use client";

/**
 * Mobile navigation drawer for the dashboard (#808).
 *
 * The sidebar is `fixed w-60` and the content pane offsets by `ml-60`, which
 * below ~768px leaves the nav covering the page with no way to dismiss it.
 * This renders the same nav as an off-canvas drawer behind a hamburger button,
 * and the sidebar itself is hidden at that breakpoint.
 *
 * The issue suggests Radix Dialog. Radix is not a dependency of this app and
 * the drawer needs exactly one dialog, so the modal behaviour Radix would
 * provide — focus trap, Escape to close, background made inert, focus returned
 * to the trigger — is implemented directly here rather than adding a
 * dependency for a single component.
 */

import { useCallback, useEffect, useRef } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import { DASHBOARD_NAV } from "@/lib/nav";

export interface MobileNavDrawerProps {
  open: boolean;
  onClose: () => void;
}

/** Focusable elements inside the drawer, in tab order. */
const FOCUSABLE =
  'a[href], button:not([disabled]), [tabindex]:not([tabindex="-1"])';

export default function MobileNavDrawer({ open, onClose }: MobileNavDrawerProps) {
  const path = usePathname();
  const { logout } = useAuth();
  const panelRef = useRef<HTMLDivElement>(null);
  const openerRef = useRef<HTMLElement | null>(null);

  // Escape closes, and Tab cycles within the panel rather than escaping to the
  // page behind it.
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
        return;
      }
      if (e.key !== "Tab" || !panelRef.current) return;

      const items = Array.from(
        panelRef.current.querySelectorAll<HTMLElement>(FOCUSABLE),
      );
      if (items.length === 0) return;

      const first = items[0];
      const last = items[items.length - 1];

      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    },
    [onClose],
  );

  useEffect(() => {
    if (!open) return;

    openerRef.current = document.activeElement as HTMLElement | null;
    document.addEventListener("keydown", handleKeyDown);

    // The page behind must not scroll under the open drawer.
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";

    // Move focus in so a keyboard or screen-reader user lands in the drawer.
    const focusTimer = requestAnimationFrame(() => {
      panelRef.current?.querySelector<HTMLElement>(FOCUSABLE)?.focus();
    });

    return () => {
      cancelAnimationFrame(focusTimer);
      document.removeEventListener("keydown", handleKeyDown);
      document.body.style.overflow = previousOverflow;
      // Returning focus to the hamburger stops a keyboard user being dumped at
      // the top of the document each time they close the menu.
      if (openerRef.current?.isConnected) openerRef.current.focus();
    };
  }, [open, handleKeyDown]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 md:hidden" data-testid="mobile-nav-drawer">
      {/* Scrim — tapping outside the panel closes the drawer. */}
      <div
        className="absolute inset-0 bg-slate-900/60"
        onClick={onClose}
        data-testid="mobile-nav-scrim"
        aria-hidden="true"
      />

      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label="Dashboard navigation"
        className="absolute inset-y-0 left-0 flex w-64 max-w-[80%] flex-col bg-slate-900 text-white shadow-xl"
      >
        <div className="flex items-center justify-between border-b border-slate-700 px-5 py-4">
          <div>
            <span className="text-lg font-bold tracking-tight">⚡ Zaps</span>
            <p className="text-xs text-slate-400 mt-0.5">Merchant Dashboard</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close navigation menu"
            className="rounded-lg p-2 text-slate-300 hover:bg-slate-800 hover:text-white transition-colors"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              aria-hidden="true"
            >
              <path d="M18 6 6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <nav className="flex-1 space-y-1 overflow-y-auto px-3 py-4">
          {DASHBOARD_NAV.map(({ href, label, icon }) => (
            <Link
              key={href}
              href={href}
              // Following a link must dismiss the drawer, or the destination
              // renders underneath it.
              onClick={onClose}
              aria-current={path === href ? "page" : undefined}
              className={`flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors ${
                path === href
                  ? "bg-indigo-600 text-white"
                  : "text-slate-300 hover:bg-slate-800 hover:text-white"
              }`}
            >
              <span aria-hidden="true">{icon}</span>
              {label}
            </Link>
          ))}
        </nav>

        <div className="border-t border-slate-700 px-3 py-4">
          <button
            type="button"
            onClick={() => {
              onClose();
              logout();
            }}
            className="w-full rounded-lg px-3 py-2 text-left text-sm text-slate-400 hover:bg-slate-800 hover:text-white transition-colors"
          >
            🚪 Sign out
          </button>
        </div>
      </div>
    </div>
  );
}
