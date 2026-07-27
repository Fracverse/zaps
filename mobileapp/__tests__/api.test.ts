import AsyncStorage from "@react-native-async-storage/async-storage";
import {
  completePrivySignup,
  linkPrivyAddress,
  stellarAddressFromPrivyPublicKey,
} from "../src/services/api";

jest.mock("@react-native-async-storage/async-storage", () => ({
  __esModule: true,
  default: jest.requireActual(
    "@react-native-async-storage/async-storage/jest/async-storage-mock"
  ),
}));

jest.mock("@stellar/stellar-sdk", () =>
  jest.requireActual(
    `${process.cwd()}/node_modules/@stellar/stellar-sdk/lib/cjs/base/strkey.js`
  )
);

const mockAsyncStorage = AsyncStorage as jest.Mocked<typeof AsyncStorage>;
const mockFetch = jest.fn();

global.fetch = mockFetch as unknown as typeof fetch;

function jsonResponse(body: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: jest.fn().mockResolvedValue(body),
  } as unknown as Response;
}

describe("Privy signup API service", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockFetch.mockReset();
  });

  it("converts a Privy Ed25519 base58 key to a Stellar address", () => {
    expect(
      stellarAddressFromPrivyPublicKey("11111111111111111111111111111111")
    ).toBe("GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF");
  });

  it("creates a wallet, proves ownership, and links the Privy identity", async () => {
    const provider = {
      _publicKey: "11111111111111111111111111111111",
      request: jest.fn().mockResolvedValue({ signature: "signed-challenge" }),
    };
    const create = jest.fn().mockResolvedValue(provider);

    mockFetch
      .mockResolvedValueOnce(jsonResponse({ challenge: "register-me" }))
      .mockResolvedValueOnce(
        jsonResponse({ token: "proof-token", username: "u_test" })
      )
      .mockResolvedValueOnce(
        jsonResponse(
          {
            token: "zaps-token",
            username: "u_test",
            privy_did: "did:privy:test-user",
          },
          201
        )
      );

    await expect(
      completePrivySignup({
        privyToken: "privy-access-token",
        privyDid: "did:privy:test-user",
        wallet: { create },
      })
    ).resolves.toEqual({
      token: "zaps-token",
      username: "u_test",
      privyDid: "did:privy:test-user",
      stellarAddress:
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    });

    expect(create).toHaveBeenCalledTimes(1);
    expect(provider.request).toHaveBeenCalledWith({
      method: "signMessage",
      params: {
        message: Buffer.from("register-me", "utf8").toString("base64"),
      },
    });
    expect(mockFetch).toHaveBeenNthCalledWith(
      2,
      expect.stringContaining("/api/auth/verify"),
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          address: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
          signature: "signed-challenge",
          challenge: "register-me",
        }),
      })
    );
    expect(mockFetch).toHaveBeenNthCalledWith(
      3,
      expect.stringContaining("/api/auth/privy"),
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          privy_token: "privy-access-token",
          privy_did: "did:privy:test-user",
          stellar_address:
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        }),
      })
    );
    expect(mockAsyncStorage.setItem).toHaveBeenCalledWith(
      "auth_token",
      "zaps-token"
    );
  });

  it("uses an already connected wallet without creating another one", async () => {
    const provider = {
      request: jest.fn().mockResolvedValue(new Uint8Array([1, 2, 3])),
    };
    const create = jest.fn();

    mockFetch
      .mockResolvedValueOnce(jsonResponse({ challenge: "challenge" }))
      .mockResolvedValueOnce(
        jsonResponse({ token: "proof-token", username: "u_test" })
      )
      .mockResolvedValueOnce(
        jsonResponse(
          {
            token: "zaps-token",
            username: "u_test",
            privy_did: "did:privy:test-user",
          },
          201
        )
      );

    await completePrivySignup({
      privyToken: "privy-access-token",
      privyDid: "did:privy:test-user",
      wallet: {
        create,
        wallets: [
          {
            publicKey: "11111111111111111111111111111111",
            getProvider: jest.fn().mockResolvedValue(provider),
          },
        ],
      },
    });

    expect(create).not.toHaveBeenCalled();
    expect(mockFetch).toHaveBeenNthCalledWith(
      2,
      expect.any(String),
      expect.objectContaining({
        body: expect.stringContaining('"signature":"AQID"'),
      })
    );
  });

  it("surfaces backend errors and does not cache a rejected token", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ error: "Privy token verification failed" }, 401)
    );

    await expect(
      linkPrivyAddress({
        privyToken: "invalid-token",
        privyDid: "did:privy:test-user",
        stellarAddress:
          "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
      })
    ).rejects.toThrow("Privy token verification failed");

    expect(mockAsyncStorage.setItem).not.toHaveBeenCalled();
  });
});
