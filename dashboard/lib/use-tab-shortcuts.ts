"use client";

import { useCallback, useEffect, useRef } from "react";

/** How long after a bare `g` the second key still counts (#1041). */
export const TAB_SHORTCUT_WINDOW_MS = 1000;

/** Typing in a form field must never trigger navigation shortcuts. */
export function isTypingTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)
  );
}

/**
 * #1041 — `g` then a tab key jumps between dashboard tabs, e.g. `g t` for
 * Transactions. `items` maps the second key to a route via `shortcut`.
 */
export function useTabShortcuts(
  items: { href: string; shortcut: string }[],
  onNavigate: (href: string) => void,
  disabled = false,
) {
  const pendingGRef = useRef(0);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (disabled || e.metaKey || e.ctrlKey || e.altKey || isTypingTarget(e.target)) return;
      const key = e.key.toLowerCase();
      if (Date.now() - pendingGRef.current < TAB_SHORTCUT_WINDOW_MS) {
        pendingGRef.current = 0;
        const item = items.find((nav) => nav.shortcut === key);
        if (item) {
          e.preventDefault();
          onNavigate(item.href);
        }
        return;
      }
      if (key === "g" && !e.shiftKey) pendingGRef.current = Date.now();
    },
    [items, onNavigate, disabled],
  );

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);
}
