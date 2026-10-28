import { beforeEach, describe, expect, it, vi } from "vitest";
import { isAllowed, isConnected, signTransaction } from "@stellar/freighter-api";
import { FreighterError, TESTNET_PASSPHRASE, signWithFreighter } from "./freighter";

vi.mock("@stellar/freighter-api", () => ({
  isConnected: vi.fn(),
  isAllowed: vi.fn(),
  signTransaction: vi.fn(),
}));

const mockIsConnected = vi.mocked(isConnected);
const mockIsAllowed = vi.mocked(isAllowed);
const mockSign = vi.mocked(signTransaction);

const UNSIGNED_XDR = "AAAAAgAAAABunsigned==";
const SIGNED_XDR = "AAAAAgAAAABsigned==";
const SIGNER = "GDSIGNERADDRESSXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";

describe("signWithFreighter", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockIsConnected.mockResolvedValue({ isConnected: true });
    mockIsAllowed.mockResolvedValue({ isAllowed: true });
    mockSign.mockResolvedValue({ signedTxXdr: SIGNED_XDR, signerAddress: SIGNER });
  });

  it("passes the unsigned XDR and default testnet passphrase to Freighter", async () => {
    await signWithFreighter(UNSIGNED_XDR);

    expect(mockSign).toHaveBeenCalledTimes(1);
    expect(mockSign).toHaveBeenCalledWith(UNSIGNED_XDR, {
      networkPassphrase: TESTNET_PASSPHRASE,
    });
  });

  it("forwards a custom network passphrase and signer address", async () => {
    const passphrase = "Public Global Stellar Network ; September 2015";
    await signWithFreighter(UNSIGNED_XDR, { networkPassphrase: passphrase, address: SIGNER });

    expect(mockSign).toHaveBeenCalledWith(UNSIGNED_XDR, {
      networkPassphrase: passphrase,
      address: SIGNER,
    });
  });

  it("returns the signed XDR and signer address on success", async () => {
    await expect(signWithFreighter(UNSIGNED_XDR)).resolves.toEqual({
      signedTxXdr: SIGNED_XDR,
      signerAddress: SIGNER,
    });
  });

  it("checks connection and permission before signing", async () => {
    await signWithFreighter(UNSIGNED_XDR);

    const connectedOrder = mockIsConnected.mock.invocationCallOrder[0];
    const allowedOrder = mockIsAllowed.mock.invocationCallOrder[0];
    const signOrder = mockSign.mock.invocationCallOrder[0];
    expect(connectedOrder).toBeLessThan(allowedOrder);
    expect(allowedOrder).toBeLessThan(signOrder);
  });

  it("rejects an empty XDR without calling Freighter", async () => {
    await expect(signWithFreighter("")).rejects.toThrow("Transaction XDR is required");
    expect(mockIsConnected).not.toHaveBeenCalled();
    expect(mockSign).not.toHaveBeenCalled();
  });

  it("throws when the wallet is not connected", async () => {
    mockIsConnected.mockResolvedValue({ isConnected: false });

    await expect(signWithFreighter(UNSIGNED_XDR)).rejects.toThrow("Freighter wallet not connected");
    expect(mockSign).not.toHaveBeenCalled();
  });

  it("throws when the site is not allowed", async () => {
    mockIsAllowed.mockResolvedValue({ isAllowed: false });

    await expect(signWithFreighter(UNSIGNED_XDR)).rejects.toThrow(
      "Freighter has not authorised this site",
    );
    expect(mockSign).not.toHaveBeenCalled();
  });

  it("surfaces a connection check error", async () => {
    mockIsConnected.mockResolvedValue({
      isConnected: false,
      error: { code: -1, message: "Extension unavailable" },
    } as Awaited<ReturnType<typeof isConnected>>);

    await expect(signWithFreighter(UNSIGNED_XDR)).rejects.toThrow("Extension unavailable");
  });

  it("maps a user rejection returned by Freighter to a FreighterError", async () => {
    mockSign.mockResolvedValue({
      signedTxXdr: "",
      signerAddress: "",
      error: { code: -4, message: "User declined access" },
    } as Awaited<ReturnType<typeof signTransaction>>);

    const err = await signWithFreighter(UNSIGNED_XDR).catch((e) => e);
    expect(err).toBeInstanceOf(FreighterError);
    expect(err.message).toBe("User declined access");
    expect(err.code).toBe(-4);
  });

  it("handles string errors returned by older Freighter versions", async () => {
    mockSign.mockResolvedValue({
      signedTxXdr: "",
      signerAddress: "",
      error: "User declined",
    } as unknown as Awaited<ReturnType<typeof signTransaction>>);

    await expect(signWithFreighter(UNSIGNED_XDR)).rejects.toThrow("User declined");
  });

  it("wraps exceptions thrown by signTransaction", async () => {
    mockSign.mockRejectedValue(new Error("Extension crashed"));

    await expect(signWithFreighter(UNSIGNED_XDR)).rejects.toBeInstanceOf(FreighterError);
    await expect(signWithFreighter(UNSIGNED_XDR)).rejects.toThrow("Extension crashed");
  });

  it("throws when Freighter returns an empty signed XDR", async () => {
    mockSign.mockResolvedValue({ signedTxXdr: "", signerAddress: SIGNER });

    await expect(signWithFreighter(UNSIGNED_XDR)).rejects.toThrow(
      "Freighter returned an empty signed transaction",
    );
  });
});
