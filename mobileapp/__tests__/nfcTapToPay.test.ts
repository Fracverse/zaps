jest.mock("react-native-nfc-manager", () => ({
  NfcEvents: { DiscoverTag: "DiscoverTag" },
  NfcTech: { Ndef: "Ndef" },
  Ndef: {
    uriRecord: (uri: string) => ({ uri }),
    encodeMessage: (records: unknown[]) => records,
  },
  default: {},
}));

import { formatNfcPaymentPayload } from "../src/services/nfcTapToPay";

describe("formatNfcPaymentPayload", () => {
  it("formats a wallet address and amount as a URI and NDEF message", () => {
    const result = formatNfcPaymentPayload({
      destination: "GABC",
      amount: "12.50",
      assetCode: "USDC",
      assetIssuer: "GISSUER",
    });

    expect(result.uri).toBe(
      "zaps://pay?address=GABC&amount=12.50&asset_code=USDC&asset_issuer=GISSUER"
    );
    expect(result.ndefMessage).toEqual([{ uri: result.uri }]);
  });

  it.each(["", "0", "-1", "1e3"])("rejects invalid amount %s", (amount) => {
    expect(() => formatNfcPaymentPayload({ destination: "GABC", amount })).toThrow();
  });
});
