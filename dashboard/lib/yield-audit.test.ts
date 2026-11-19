import { describe, expect, it } from "vitest";
import type { YieldAuditLogItem } from "./api";
import { mapAuditItem, reconcileAuditRows, type YieldTx } from "./yield-audit";

const item = (id: string, txHash: string, type = "deposit"): YieldAuditLogItem => ({
  id,
  tx_hash: txHash,
  type,
  amount: 50_000_000,
  created_at: "2026-11-18T10:00:00Z",
  address: "GABC",
  asset: "USDC",
  block_height: 100,
});

const apiRow = (id: string): YieldTx => mapAuditItem(item(id, `hash-${id}`));

const pendingConfig: YieldTx = {
  id: "cfg-1",
  txHash: "",
  timestamp: "2026-11-18T11:00:00Z",
  address: "GADMIN",
  action: "config",
  tokenVolume: 0,
  asset: "—",
  pending: true,
};

describe("mapAuditItem", () => {
  it("maps API fields without inventing values", () => {
    const row = mapAuditItem({ ...item("1", "h1", "yield_distribution"), address: undefined, fee: undefined });
    expect(row).toMatchObject({ txHash: "h1", action: "yield_accrual", tokenVolume: 5, address: "" });
    expect(row.fee).toBeUndefined();
  });
});

describe("reconcileAuditRows", () => {
  it("shows only API records when the query is populated", () => {
    const rows = reconcileAuditRows([], [item("1", "h1"), item("2", "h2", "withdraw")]);
    expect(rows.map((r) => r.id)).toEqual(["1", "2"]);
  });

  it("clears previous rows when the API returns an empty list", () => {
    expect(reconcileAuditRows([apiRow("1"), apiRow("2")], [])).toEqual([]);
  });

  it("clears previous rows when the query fails", () => {
    expect(reconcileAuditRows([apiRow("1")], null)).toEqual([]);
  });

  it("keeps pending local entries across empty and failed queries", () => {
    expect(reconcileAuditRows([pendingConfig, apiRow("1")], [])).toEqual([pendingConfig]);
    expect(reconcileAuditRows([pendingConfig], null)).toEqual([pendingConfig]);
  });

  it("drops a pending entry once the API returns its transaction", () => {
    const pending = { ...pendingConfig, txHash: "h9" };
    const rows = reconcileAuditRows([pending], [item("9", "h9", "config")]);
    expect(rows).toHaveLength(1);
    expect(rows[0].pending).toBeUndefined();
  });

  it("never keeps non-pending local rows", () => {
    const local: YieldTx = { ...pendingConfig, id: "x", pending: false };
    expect(reconcileAuditRows([local], [item("1", "h1")]).map((r) => r.id)).toEqual(["1"]);
  });
});
