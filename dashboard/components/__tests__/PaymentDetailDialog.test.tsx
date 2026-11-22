import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React from "react";
import * as StellarSdk from "@stellar/stellar-sdk";
import PaymentDetailDialog from "../PaymentDetailDialog";

vi.mock("@/lib/api", () => ({
  api: { userPayments: vi.fn().mockResolvedValue([]) },
}));

function buildEnvelopeXdr(memo: string): { xdr: string; source: string } {
  const source = StellarSdk.Keypair.random().publicKey();
  const destination = StellarSdk.Keypair.random().publicKey();
  const tx = new StellarSdk.TransactionBuilder(new StellarSdk.Account(source, "41"), {
    fee: "100",
    networkPassphrase: StellarSdk.Networks.TESTNET,
  })
    .addOperation(
      StellarSdk.Operation.payment({
        destination,
        asset: StellarSdk.Asset.native(),
        amount: "12.5",
      }),
    )
    .addMemo(StellarSdk.Memo.text(memo))
    .setTimeout(0)
    .build();
  return { xdr: tx.toXDR(), source };
}

describe("PaymentDetailDialog XDR inspector (#1029)", () => {
  it("decodes a transaction envelope into a summary and tree view", () => {
    const { xdr, source } = buildEnvelopeXdr("payout-7");
    render(<PaymentDetailDialog xdr={xdr} onClose={() => {}} />);

    expect(screen.getByText("Transaction Details")).toBeInTheDocument();
    expect(screen.getByText("TransactionEnvelope")).toBeInTheDocument();
    expect(screen.getByText("TESTNET")).toBeInTheDocument();
    expect(screen.getAllByText(source).length).toBeGreaterThan(0);
    expect(screen.getAllByText("42").length).toBeGreaterThan(0);
    expect(screen.getAllByText(/payout-7/).length).toBeGreaterThan(0);
    expect(screen.getByText("Operations (1)")).toBeInTheDocument();
    expect(screen.getAllByText("payment").length).toBeGreaterThan(0);
    expect(screen.getByText("Decoded Structure")).toBeInTheDocument();
  });

  it("decodes a bare ScVal payload without a transaction summary", () => {
    const xdr = StellarSdk.nativeToScVal(7, { type: "u32" }).toXDR("base64");
    render(<PaymentDetailDialog xdr={xdr} onClose={() => {}} />);

    expect(screen.queryByText("Transaction Summary")).not.toBeInTheDocument();
    expect(screen.getByText("Decoded Structure")).toBeInTheDocument();
  });

  it("shows an error panel instead of crashing on undecodable XDR", () => {
    render(<PaymentDetailDialog xdr="not-valid-xdr" onClose={() => {}} />);

    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Failed to decode XDR");
    expect(screen.queryByText("Decoded Structure")).not.toBeInTheDocument();
  });

  it("closes on Escape and on the close button", () => {
    const onClose = vi.fn();
    const { xdr } = buildEnvelopeXdr("close-me");
    render(<PaymentDetailDialog xdr={xdr} onClose={onClose} />);

    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: "Close dialog" }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
