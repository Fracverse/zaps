// #1040 — dark/light theme toggling: the `dark` class is applied to the root
// <html> element and the selection is persisted in localStorage.
//
// The production wiring lives in app/layout.tsx (next-themes <ThemeProvider>);
// these tests exercise the same provider contract that layout.tsx relies on,
// plus the ThemeSelector control that drives it from the dashboard navbar.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import React from "react";
import { ThemeProvider } from "next-themes";
import ThemeSelector from "@/components/ThemeSelector";

function renderWithTheme(ui: React.ReactNode) {
  return render(
    <ThemeProvider
      attribute="class"
      defaultTheme="system"
      enableSystem
      storageKey="theme"
    >
      {ui}
    </ThemeProvider>
  );
}

describe("theme provider (#1040)", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.classList.remove("dark", "light");
    // jsdom has no matchMedia; next-themes calls it to resolve "system".
    Object.defineProperty(window, "matchMedia", {
      writable: true,
      configurable: true,
      value: (query: string) => ({
        matches: false,
        media: query,
        onchange: null,
        addListener: () => {},
        removeListener: () => {},
        addEventListener: () => {},
        removeEventListener: () => {},
        dispatchEvent: () => false,
      }),
    });
  });

  afterEach(() => {
    localStorage.clear();
    document.documentElement.classList.remove("dark", "light");
  });

  it("applies the `dark` class to the root <html> element for the dark theme", async () => {
    renderWithTheme(<ThemeSelector />);

    const select = await screen.findByRole("combobox");
    await act(async () => {
      fireEvent.change(select, { target: { value: "dark" } });
    });

    await waitFor(() => {
      expect(document.documentElement.classList.contains("dark")).toBe(true);
    });
  });

  it("toggles back to the light theme by removing the `dark` class", async () => {
    renderWithTheme(<ThemeSelector />);

    const select = await screen.findByRole("combobox");
    await act(async () => {
      fireEvent.change(select, { target: { value: "dark" } });
    });
    await waitFor(() => {
      expect(document.documentElement.classList.contains("dark")).toBe(true);
    });

    await act(async () => {
      fireEvent.change(select, { target: { value: "light" } });
    });
    await waitFor(() => {
      expect(document.documentElement.classList.contains("dark")).toBe(false);
    });
  });

  it("persists the selected theme in localStorage", async () => {
    renderWithTheme(<ThemeSelector />);

    const select = await screen.findByRole("combobox");
    await act(async () => {
      fireEvent.change(select, { target: { value: "dark" } });
    });

    await waitFor(() => {
      expect(localStorage.getItem("theme")).toBe("dark");
    });
  });

  it("restores a previously persisted theme on mount", async () => {
    localStorage.setItem("theme", "dark");

    renderWithTheme(<ThemeSelector />);

    await waitFor(() => {
      expect(document.documentElement.classList.contains("dark")).toBe(true);
    });
  });
});
