import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import BridgeDepositMonitor, { BridgeStatusBadge, explorerTxUrl } from "./BridgeDepositMonitor";
import { BridgeDeposit, BridgeDepositStatus } from "@/lib/api";

const DEPOSITS: BridgeDeposit[] = [
  {
    id: "dep-1",
    source_chain: "ethereum",
    target_chain: "stellar",
    asset: "USDC",
    amount: "250.00",
    bridge_tx_hash: "0xabc1234567890def",
    target_tx_hash: "f00dbabe12345678",
    status: "confirmed",
    created_at: "2026-10-10T09:00:00Z",
  },
  {
    id: "dep-2",
    source_chain: "solana",
    target_chain: "stellar",
    asset: "USDC",
    amount: "40.00",
    bridge_tx_hash: "5Sol9xyz",
    status: "pending",
    created_at: "2026-10-10T09:05:00Z",
  },
  {
    id: "dep-3",
    source_chain: "unknownchain",
    target_chain: "stellar",
    asset: "USDT",
    amount: "12.50",
    bridge_tx_hash: "deadbeef",
    status: "failed",
    created_at: "2026-10-10T09:10:00Z",
  },
];

describe("BridgeStatusBadge", () => {
  it.each<[BridgeDepositStatus, string, string]>([
    ["pending", "Pending", "bg-yellow-100"],
    ["confirmed", "Confirmed", "bg-green-100"],
    ["failed", "Failed", "bg-red-100"],
  ])("renders the %s variant", (status, label, cls) => {
    render(<BridgeStatusBadge status={status} />);
    const badge = screen.getByText(label);
    expect(badge).toHaveAttribute("data-status", status);
    expect(badge.className).toContain(cls);
  });
});

describe("explorerTxUrl", () => {
  it("builds explorer links for known chains, case-insensitively", () => {
    expect(explorerTxUrl("Ethereum", "0x1")).toBe("https://etherscan.io/tx/0x1");
    expect(explorerTxUrl("stellar", "ab")).toBe("https://stellar.expert/explorer/public/tx/ab");
  });

  it("returns null for unknown chains", () => {
    expect(explorerTxUrl("unknownchain", "x")).toBeNull();
  });
});

describe("BridgeDepositMonitor", () => {
  it("renders skeletons before the first load completes", () => {
    render(<BridgeDepositMonitor deposits={null} loading />);
    expect(screen.queryAllByTestId("bridge-deposit-row")).toHaveLength(0);
    expect(screen.queryByText("No incoming bridge deposits")).not.toBeInTheDocument();
  });

  it("renders an empty state", () => {
    render(<BridgeDepositMonitor deposits={[]} loading={false} />);
    expect(screen.getByText("No incoming bridge deposits")).toBeInTheDocument();
  });

  it("renders deposit details with the correct badge per row", () => {
    render(<BridgeDepositMonitor deposits={DEPOSITS} loading={false} />);
    const rows = screen.getAllByTestId("bridge-deposit-row");
    expect(rows).toHaveLength(3);

    expect(within(rows[0]).getByText("ethereum → stellar")).toBeInTheDocument();
    expect(within(rows[0]).getByText("250.00 USDC")).toBeInTheDocument();
    expect(within(rows[0]).getByTestId("bridge-status-badge")).toHaveAttribute("data-status", "confirmed");
    expect(within(rows[1]).getByTestId("bridge-status-badge")).toHaveAttribute("data-status", "pending");
    expect(within(rows[2]).getByTestId("bridge-status-badge")).toHaveAttribute("data-status", "failed");
  });

  it("links bridge hashes to source and destination explorers", () => {
    render(<BridgeDepositMonitor deposits={DEPOSITS} loading={false} />);
    const rows = screen.getAllByTestId("bridge-deposit-row");

    expect(within(rows[0]).getByText("0xabc123…9def").closest("a")).toHaveAttribute(
      "href",
      "https://etherscan.io/tx/0xabc1234567890def",
    );
    expect(within(rows[0]).getByText("(dest)")).toHaveAttribute(
      "href",
      "https://stellar.expert/explorer/public/tx/f00dbabe12345678",
    );
    expect(within(rows[1]).getByText("5Sol9xyz")).toHaveAttribute(
      "href",
      "https://solscan.io/tx/5Sol9xyz",
    );
    // Unknown chain: hash rendered as plain text
    expect(within(rows[2]).getByText("deadbeef").closest("a")).toBeNull();
  });

  it("links each row to its backend bridge log endpoint", () => {
    render(<BridgeDepositMonitor deposits={DEPOSITS} loading={false} />);
    const links = screen.getAllByRole("link", { name: "View logs" });
    expect(links).toHaveLength(3);
    expect(links[0].getAttribute("href")).toMatch(/\/api\/v1\/admin\/bridge\/deposits\/dep-1\/logs$/);
  });

  it("shows per-status counts in the summary", () => {
    render(<BridgeDepositMonitor deposits={DEPOSITS} loading={false} />);
    expect(screen.getAllByTestId("bridge-status-badge")).toHaveLength(6);
  });
});
