// #785 — Tests for HealthStatusIndicator
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import React from "react";

const mockHealthFn = vi.fn();

vi.mock("@/lib/api", () => ({
  api: { health: (...a: unknown[]) => mockHealthFn(...a) },
}));

import HealthStatusIndicator from "@/components/HealthStatusIndicator";

const OK_RESPONSE = {
  status: "ok" as const,
  components: {
    database: { status: "ok", latency_ms: 3 },
    yield_db: { status: "ok", latency_ms: 5 },
    soroban_rpc: { status: "ok", latency_ms: 42 },
  },
  checked_at: new Date().toISOString(),
};

const DEGRADED_RESPONSE = {
  status: "degraded" as const,
  components: {
    database: { status: "ok", latency_ms: 3 },
    yield_db: { status: "error", latency_ms: 150 },
    soroban_rpc: { status: "ok", latency_ms: 42 },
  },
  checked_at: new Date().toISOString(),
};

describe("HealthStatusIndicator", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the health button", async () => {
    mockHealthFn.mockResolvedValue(OK_RESPONSE);
    render(<HealthStatusIndicator />);
    expect(screen.getByTestId("health-status-btn")).toBeTruthy();
  });

  it("shows 'All systems OK' when health status is ok", async () => {
    mockHealthFn.mockResolvedValue(OK_RESPONSE);
    render(<HealthStatusIndicator />);
    await waitFor(() => {
      expect(screen.getByText("All systems OK")).toBeTruthy();
    });
  });

  it("shows 'Degraded' when health status is degraded", async () => {
    mockHealthFn.mockResolvedValue(DEGRADED_RESPONSE);
    render(<HealthStatusIndicator />);
    await waitFor(() => {
      expect(screen.getByText("Degraded")).toBeTruthy();
    });
  });

  it("opens the detail panel on click and shows per-component labels", async () => {
    mockHealthFn.mockResolvedValue(OK_RESPONSE);
    render(<HealthStatusIndicator />);

    await waitFor(() => screen.getByText("All systems OK"));

    const btn = screen.getByTestId("health-status-btn");
    await userEvent.click(btn);

    const panel = screen.getByTestId("health-status-panel");
    expect(panel).toBeTruthy();
    expect(panel.textContent).toContain("DB");
    expect(panel.textContent).toContain("Yield DB");
    expect(panel.textContent).toContain("RPC");
  });

  it("shows latencies in the panel", async () => {
    mockHealthFn.mockResolvedValue(OK_RESPONSE);
    render(<HealthStatusIndicator />);

    await waitFor(() => screen.getByText("All systems OK"));
    await userEvent.click(screen.getByTestId("health-status-btn"));

    const panel = screen.getByTestId("health-status-panel");
    expect(panel.textContent).toContain("3ms");
    expect(panel.textContent).toContain("42ms");
  });

  it("shows error message when health fetch fails", async () => {
    mockHealthFn.mockRejectedValue(new Error("Network error"));
    render(<HealthStatusIndicator />);

    await waitFor(() => screen.getByTestId("health-status-btn"));
    await userEvent.click(screen.getByTestId("health-status-btn"));

    await waitFor(() => {
      expect(screen.getByText(/Could not reach \/health/)).toBeTruthy();
    });
  });
});
