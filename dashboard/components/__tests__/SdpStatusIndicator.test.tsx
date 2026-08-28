// #794 — Unit tests for SDP server sync status indicator
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/react";
import React from "react";

const mockGetSdpStatusFn = vi.fn();

vi.mock("@/lib/api", () => ({
  api: {
    getSdpStatus: (...a: unknown[]) => mockGetSdpStatusFn(...a),
    sdp: {
      getStatus: (...a: unknown[]) => mockGetSdpStatusFn(...a),
    },
  },
}));

import { SdpStatusIndicator } from "@/app/dashboard/payouts/page";

const OPERATIONAL_RESPONSE = {
  status: "operational",
  version: "v2.4.0",
  network: "testnet",
  synced_at: new Date().toISOString(),
  latest_ledger: 12345678,
};

const SYNCED_RESPONSE = {
  status: "synced",
  version: "v2.4.0",
  network: "mainnet",
  synced_at: new Date().toISOString(),
  latest_ledger: 87654321,
};

const SYNCING_RESPONSE = {
  status: "syncing",
  version: "v2.4.0",
  network: "testnet",
};

const DEGRADED_RESPONSE = {
  status: "degraded",
  version: "v2.4.0",
};

describe("SdpStatusIndicator (#794)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("renders the SDP indicator element", async () => {
    mockGetSdpStatusFn.mockResolvedValue(OPERATIONAL_RESPONSE);
    render(<SdpStatusIndicator />);
    expect(screen.getByTestId("sdp-status-indicator")).toBeTruthy();
  });

  it("shows 'Synced' badge when SDP status is operational", async () => {
    mockGetSdpStatusFn.mockResolvedValue(OPERATIONAL_RESPONSE);
    render(<SdpStatusIndicator />);
    await waitFor(() => {
      expect(screen.getByText("Synced")).toBeTruthy();
    });
  });

  it("shows 'Synced' badge when SDP status is synced", async () => {
    mockGetSdpStatusFn.mockResolvedValue(SYNCED_RESPONSE);
    render(<SdpStatusIndicator />);
    await waitFor(() => {
      expect(screen.getByText("Synced")).toBeTruthy();
    });
  });

  it("shows 'Syncing' badge when SDP status is syncing", async () => {
    mockGetSdpStatusFn.mockResolvedValue(SYNCING_RESPONSE);
    render(<SdpStatusIndicator />);
    await waitFor(() => {
      expect(screen.getByText("Syncing")).toBeTruthy();
    });
  });

  it("shows 'Degraded' badge when SDP status is degraded", async () => {
    mockGetSdpStatusFn.mockResolvedValue(DEGRADED_RESPONSE);
    render(<SdpStatusIndicator />);
    await waitFor(() => {
      expect(screen.getByText("Degraded")).toBeTruthy();
    });
  });

  it("shows 'Offline' badge when SDP status fetch fails", async () => {
    mockGetSdpStatusFn.mockRejectedValue(new Error("Connection refused"));
    render(<SdpStatusIndicator />);
    await waitFor(() => {
      expect(screen.getByText("Offline")).toBeTruthy();
    });
  });

  it("starts polling and clears the interval on unmount", async () => {
    mockGetSdpStatusFn.mockResolvedValue(OPERATIONAL_RESPONSE);
    const setIntervalSpy = vi.spyOn(globalThis, "setInterval");
    const clearIntervalSpy = vi.spyOn(globalThis, "clearInterval");

    const { unmount } = render(<SdpStatusIndicator />);

    expect(setIntervalSpy).toHaveBeenCalled();

    expect(clearIntervalSpy).not.toHaveBeenCalled();

    unmount();

    expect(clearIntervalSpy).toHaveBeenCalled();
  });
});
