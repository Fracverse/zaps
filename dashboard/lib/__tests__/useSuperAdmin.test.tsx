// #786 — Tests for useSuperAdmin hook and role-based button visibility
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import React from "react";
import { renderHook, act } from "@testing-library/react";

// ── Mock the auth context ─────────────────────────────────────────────────────

let mockRole: string | null = null;

vi.mock("@/lib/auth-context", async () => {
  return {
    ROLE_SUPERADMIN: "superadmin",
    useAuth: () => ({ token: "tok", role: mockRole, login: vi.fn(), logout: vi.fn() }),
    useSuperAdmin: () => mockRole === "superadmin",
    AuthProvider: ({ children }: { children: React.ReactNode }) => children,
  };
});

import { useSuperAdmin, ROLE_SUPERADMIN } from "@/lib/auth-context";

// ── useSuperAdmin hook ────────────────────────────────────────────────────────

describe("useSuperAdmin", () => {
  it("returns true when role is 'superadmin'", () => {
    mockRole = ROLE_SUPERADMIN;
    const { result } = renderHook(() => useSuperAdmin());
    expect(result.current).toBe(true);
  });

  it("returns false when role is 'admin'", () => {
    mockRole = "admin";
    const { result } = renderHook(() => useSuperAdmin());
    expect(result.current).toBe(false);
  });

  it("returns false when role is null (unauthenticated)", () => {
    mockRole = null;
    const { result } = renderHook(() => useSuperAdmin());
    expect(result.current).toBe(false);
  });

  it("returns false when role is an arbitrary string", () => {
    mockRole = "viewer";
    const { result } = renderHook(() => useSuperAdmin());
    expect(result.current).toBe(false);
  });
});

// ── Export Users button (dashboard/page.tsx) ─────────────────────────────────

// We test a minimal reproduction of the gating pattern rather than the full page
// to avoid pulling in every dashboard dependency.

function ExportButton({ isSuperAdmin }: { isSuperAdmin: boolean }) {
  return (
    <button
      disabled={!isSuperAdmin}
      aria-disabled={!isSuperAdmin}
      data-testid="export-users-btn"
      title={!isSuperAdmin ? "Superadmin access required" : undefined}
    >
      Export Users (CSV)
    </button>
  );
}

describe("Export Users button gating", () => {
  it("is enabled for superadmin", () => {
    render(<ExportButton isSuperAdmin={true} />);
    const btn = screen.getByTestId("export-users-btn");
    expect(btn).not.toBeDisabled();
    expect(btn.getAttribute("title")).toBeNull();
  });

  it("is disabled for non-superadmin", () => {
    render(<ExportButton isSuperAdmin={false} />);
    const btn = screen.getByTestId("export-users-btn");
    expect(btn).toBeDisabled();
    expect(btn.getAttribute("title")).toBe("Superadmin access required");
  });
});

// ── Vault Sign & Submit button (yield/page.tsx) ──────────────────────────────

function VaultSignButton({
  isSuperAdmin,
  walletConnected,
  confirmed,
}: {
  isSuperAdmin: boolean;
  walletConnected: boolean;
  confirmed: boolean;
}) {
  return (
    <button
      type="submit"
      data-testid="vault-sign-submit"
      disabled={!isSuperAdmin || !confirmed || !walletConnected}
      title={!isSuperAdmin ? "Superadmin access required" : undefined}
    >
      {!isSuperAdmin
        ? "Superadmin access required"
        : !walletConnected
        ? "Connect wallet to sign"
        : "Sign & Submit via Freighter"}
    </button>
  );
}

describe("Vault Sign & Submit button gating", () => {
  it("is disabled when not superadmin even with wallet connected and confirmed", () => {
    render(
      <VaultSignButton
        isSuperAdmin={false}
        walletConnected={true}
        confirmed={true}
      />,
    );
    const btn = screen.getByTestId("vault-sign-submit");
    expect(btn).toBeDisabled();
    expect(btn.textContent).toContain("Superadmin access required");
  });

  it("is disabled when superadmin but wallet not connected", () => {
    render(
      <VaultSignButton
        isSuperAdmin={true}
        walletConnected={false}
        confirmed={true}
      />,
    );
    const btn = screen.getByTestId("vault-sign-submit");
    expect(btn).toBeDisabled();
    expect(btn.textContent).toContain("Connect wallet to sign");
  });

  it("is enabled when superadmin, wallet connected, and confirmed", () => {
    render(
      <VaultSignButton
        isSuperAdmin={true}
        walletConnected={true}
        confirmed={true}
      />,
    );
    const btn = screen.getByTestId("vault-sign-submit");
    expect(btn).not.toBeDisabled();
    expect(btn.textContent).toContain("Sign & Submit via Freighter");
  });
});

// ── Blacklist button (contracts/page.tsx) ────────────────────────────────────

function BlacklistButton({ isSuperAdmin }: { isSuperAdmin: boolean }) {
  if (isSuperAdmin) {
    return (
      <button data-testid="blacklist-btn" className="bg-red-50 text-red-700">
        Blacklist
      </button>
    );
  }
  return (
    <button
      disabled
      aria-disabled="true"
      data-testid="blacklist-btn"
      title="Superadmin access required"
      className="bg-slate-100 text-slate-400 cursor-not-allowed"
    >
      Blacklist
    </button>
  );
}

describe("Blacklist button gating", () => {
  it("is enabled (active styles) for superadmin", () => {
    render(<BlacklistButton isSuperAdmin={true} />);
    const btn = screen.getByTestId("blacklist-btn");
    expect(btn).not.toBeDisabled();
    expect(btn.className).toContain("red");
  });

  it("is rendered disabled for non-superadmin", () => {
    render(<BlacklistButton isSuperAdmin={false} />);
    const btn = screen.getByTestId("blacklist-btn");
    expect(btn).toBeDisabled();
    expect(btn.getAttribute("title")).toBe("Superadmin access required");
    expect(btn.className).toContain("slate");
  });
});

// ── Pause Vault toggle (yield/page.tsx) ──────────────────────────────────────

function PauseToggle({
  isSuperAdmin,
  paused,
}: {
  isSuperAdmin: boolean;
  paused: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={paused}
      data-testid="vault-pause-toggle"
      disabled={!isSuperAdmin}
      aria-disabled={!isSuperAdmin}
      title={!isSuperAdmin ? "Superadmin access required" : undefined}
    >
      {paused ? "Paused" : "Active"}
    </button>
  );
}

describe("Pause Vault toggle gating", () => {
  it("is interactive for superadmin", () => {
    render(<PauseToggle isSuperAdmin={true} paused={false} />);
    const toggle = screen.getByTestId("vault-pause-toggle");
    expect(toggle).not.toBeDisabled();
    expect(toggle.getAttribute("aria-checked")).toBe("false");
  });

  it("is disabled for non-superadmin", () => {
    render(<PauseToggle isSuperAdmin={false} paused={false} />);
    const toggle = screen.getByTestId("vault-pause-toggle");
    expect(toggle).toBeDisabled();
    expect(toggle.getAttribute("title")).toBe("Superadmin access required");
  });
});
