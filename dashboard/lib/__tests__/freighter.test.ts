import { describe, it, expect, beforeEach, vi } from "vitest";

const isConnected = vi.fn();
const getAddress = vi.fn();
const getNetwork = vi.fn();
const requestAccess = vi.fn();
const signTransaction = vi.fn();

// The adapter imports the package dynamically; mocking the module id covers
// both that path and anything re-exported from it.
vi.mock("@stellar/freighter-api", () => ({
  isConnected: (...a: unknown[]) => isConnected(...a),
  getAddress: (...a: unknown[]) => getAddress(...a),
  getNetwork: (...a: unknown[]) => getNetwork(...a),
  requestAccess: (...a: unknown[]) => requestAccess(...a),
  signTransaction: (...a: unknown[]) => signTransaction(...a),
}));

import {
  DEFAULT_WALLET_STATE,
  connectFreighter,
  detectFreighter,
  signWithFreighter,
  truncateKey,
} from "@/lib/freighter";

const XDR = "AAAAAgAAAABmockEnvelopeXDR==";
const PASSPHRASE = "Test SDF Network ; September 2015";
const PUBKEY = "GD3XABCDEFGHIJKLMNOPQRSTUVWXYZ12345678ABCD";

describe("freighter adapter", () => {
  beforeEach(() => {
    isConnected.mockReset();
    getAddress.mockReset();
    getNetwork.mockReset();
    requestAccess.mockReset();
    signTransaction.mockReset();
  });

  describe("signWithFreighter", () => {
    it("passes the transaction XDR and network passphrase through unchanged", async () => {
      signTransaction.mockResolvedValue({ signedTxXdr: "signed-xdr" });

      await signWithFreighter(XDR, PASSPHRASE);

      expect(signTransaction).toHaveBeenCalledTimes(1);
      expect(signTransaction).toHaveBeenCalledWith(XDR, {
        networkPassphrase: PASSPHRASE,
      });
    });

    it("returns the signed envelope from the wallet", async () => {
      signTransaction.mockResolvedValue({ signedTxXdr: "signed-xdr" });

      await expect(signWithFreighter(XDR, PASSPHRASE)).resolves.toEqual({
        signedTxXdr: "signed-xdr",
      });
    });

    it("throws with the wallet's message when the user rejects the request", async () => {
      signTransaction.mockResolvedValue({ error: "User declined access" });

      await expect(signWithFreighter(XDR, PASSPHRASE)).rejects.toThrow(
        "User declined access",
      );
    });

    it("does not treat an absent error field as a failure", async () => {
      signTransaction.mockResolvedValue({ signedTxXdr: "signed-xdr", error: undefined });

      await expect(signWithFreighter(XDR, PASSPHRASE)).resolves.toEqual({
        signedTxXdr: "signed-xdr",
        error: undefined,
      });
    });

    it("propagates a thrown extension error", async () => {
      signTransaction.mockRejectedValue(new Error("Extension unavailable"));

      await expect(signWithFreighter(XDR, PASSPHRASE)).rejects.toThrow(
        "Extension unavailable",
      );
    });

    it("keeps each signature request independent", async () => {
      signTransaction
        .mockResolvedValueOnce({ signedTxXdr: "first" })
        .mockResolvedValueOnce({ signedTxXdr: "second" });

      const a = await signWithFreighter("xdr-a", PASSPHRASE);
      const b = await signWithFreighter("xdr-b", "Public Global Stellar Network ; September 2015");

      expect(a.signedTxXdr).toBe("first");
      expect(b.signedTxXdr).toBe("second");
      expect(signTransaction.mock.calls[0][0]).toBe("xdr-a");
      expect(signTransaction.mock.calls[1][0]).toBe("xdr-b");
      expect(signTransaction.mock.calls[1][1]).toEqual({
        networkPassphrase: "Public Global Stellar Network ; September 2015",
      });
    });
  });

  describe("detectFreighter", () => {
    it("reports a connected wallet with its key and network", async () => {
      isConnected.mockResolvedValue({ isConnected: true });
      getAddress.mockResolvedValue({ address: PUBKEY });
      getNetwork.mockResolvedValue({ network: "TESTNET" });

      await expect(detectFreighter()).resolves.toEqual({
        installed: true,
        connected: true,
        publicKey: PUBKEY,
        network: "TESTNET",
      });
    });

    it("reports installed-but-disconnected without asking for the address", async () => {
      isConnected.mockResolvedValue({ isConnected: false });

      await expect(detectFreighter()).resolves.toEqual({
        installed: true,
        connected: false,
        publicKey: null,
        network: null,
      });
      expect(getAddress).not.toHaveBeenCalled();
    });

    it("still reports connected when only the network lookup fails", async () => {
      isConnected.mockResolvedValue({ isConnected: true });
      getAddress.mockResolvedValue({ address: PUBKEY });
      getNetwork.mockRejectedValue(new Error("no network"));

      await expect(detectFreighter()).resolves.toEqual({
        installed: true,
        connected: true,
        publicKey: PUBKEY,
        network: null,
      });
    });

    it("falls back to the disconnected default when the extension is absent", async () => {
      isConnected.mockRejectedValue(new Error("not installed"));

      await expect(detectFreighter()).resolves.toEqual(DEFAULT_WALLET_STATE);
    });
  });

  describe("connectFreighter", () => {
    it("requests access then re-reads the wallet state", async () => {
      requestAccess.mockResolvedValue({ address: PUBKEY });
      isConnected.mockResolvedValue({ isConnected: true });
      getAddress.mockResolvedValue({ address: PUBKEY });
      getNetwork.mockResolvedValue({ network: "TESTNET" });

      const state = await connectFreighter();

      expect(requestAccess).toHaveBeenCalledTimes(1);
      expect(state.connected).toBe(true);
      expect(state.publicKey).toBe(PUBKEY);
    });

    it("throws when the user declines the connection prompt", async () => {
      requestAccess.mockResolvedValue({ error: "User declined access" });

      await expect(connectFreighter()).rejects.toThrow("User declined access");
    });

    it("wraps a non-Error rejection in an Error", async () => {
      requestAccess.mockRejectedValue("boom");

      await expect(connectFreighter()).rejects.toThrow("Failed to connect Freighter");
    });
  });

  describe("truncateKey", () => {
    it("shortens a full Stellar public key", () => {
      expect(truncateKey(PUBKEY)).toBe("GD3X…ABCD");
    });

    it("respects custom head and tail lengths", () => {
      expect(truncateKey(PUBKEY, 6, 2)).toBe("GD3XAB…CD");
    });

    it("leaves a short key untouched", () => {
      expect(truncateKey("GABC")).toBe("GABC");
    });
  });
});
