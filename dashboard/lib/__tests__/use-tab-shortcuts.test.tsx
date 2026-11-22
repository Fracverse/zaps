import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { NAV_ITEMS } from "@/components/CommandPalette";
import { TAB_SHORTCUT_WINDOW_MS, useTabShortcuts } from "@/lib/use-tab-shortcuts";

function press(key: string, init: KeyboardEventInit = {}, target: EventTarget = document.body) {
  target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, ...init }));
}

describe("useTabShortcuts (#1041)", () => {
  const onNavigate = vi.fn();

  beforeEach(() => {
    vi.useFakeTimers();
    onNavigate.mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
    document.body.innerHTML = "";
  });

  it.each(NAV_ITEMS.map((item) => [item.shortcut, item.href]))(
    "g %s navigates to %s",
    (shortcut, href) => {
      renderHook(() => useTabShortcuts(NAV_ITEMS, onNavigate));
      press("g");
      press(shortcut);
      expect(onNavigate).toHaveBeenCalledWith(href);
    },
  );

  it("ignores the second key once the window has passed", () => {
    renderHook(() => useTabShortcuts(NAV_ITEMS, onNavigate));
    press("g");
    vi.advanceTimersByTime(TAB_SHORTCUT_WINDOW_MS + 1);
    press("t");
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it("ignores keys pressed while typing in an input", () => {
    renderHook(() => useTabShortcuts(NAV_ITEMS, onNavigate));
    const input = document.createElement("input");
    document.body.appendChild(input);
    press("g", {}, input);
    press("t", {}, input);
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it("ignores sequences with modifier keys held", () => {
    renderHook(() => useTabShortcuts(NAV_ITEMS, onNavigate));
    press("g", { ctrlKey: true });
    press("t");
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it("is inert while disabled (command palette open)", () => {
    renderHook(() => useTabShortcuts(NAV_ITEMS, onNavigate, true));
    press("g");
    press("t");
    expect(onNavigate).not.toHaveBeenCalled();
  });
});
