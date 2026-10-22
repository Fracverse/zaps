import AsyncStorage from "@react-native-async-storage/async-storage";
import {
  completePrivySignup,
  linkPrivyAddress,
  stellarAddressFromPrivyPublicKey,
  resolveUsername,
  safeResolveUsername,
  batchResolveUsernames,
  resolveUsernameAsZapsUser,
  fetchReceiverForTransaction,
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

// ── Username Resolution Tests (#911) ──────────────────────────────────────────

// Mock expo-secure-store so apiFetch can call getSessionToken() without crashing.
jest.mock("expo-secure-store", () => ({
  getItemAsync: jest.fn().mockResolvedValue(null),
  setItemAsync: jest.fn().mockResolvedValue(undefined),
  deleteItemAsync: jest.fn().mockResolvedValue(undefined),
}));

// A valid Stellar public key for test fixtures.
const TEST_ADDRESS = "GA2C5RFPE6GCKMY3US5FAB5A6T6E7Q3ABYH5O4ZN7LF3K4SKNQQQ7HZB";

describe("resolveUsername (#911)", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockFetch.mockReset();
  });

  it("resolves a valid username to its Stellar address", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ username: "tolu", address: TEST_ADDRESS })
    );

    const result = await resolveUsername("tolu");
    expect(result.username).toBe("tolu");
    expect(result.address).toBe(TEST_ADDRESS);
    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringContaining("/api/users/resolve/tolu"),
      expect.objectContaining({ headers: expect.any(Object) })
    );
  });

  it("trims and lowercases the username before resolving", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ username: "tolu", address: TEST_ADDRESS })
    );

    await resolveUsername("  TOLU  ");
    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringContaining("/api/users/resolve/tolu"),
      expect.any(Object)
    );
  });

  it("throws INVALID_USERNAME for an empty string", async () => {
    await expect(resolveUsername("")).rejects.toMatchObject({
      code: "INVALID_USERNAME",
    });
    expect(mockFetch).not.toHaveBeenCalled();
  });

  it("throws INVALID_USERNAME for a whitespace-only string", async () => {
    await expect(resolveUsername("   ")).rejects.toMatchObject({
      code: "INVALID_USERNAME",
    });
  });

  it("throws INVALID_USERNAME for null/undefined input", async () => {
    await expect(resolveUsername(null as unknown as string)).rejects.toMatchObject({
      code: "INVALID_USERNAME",
    });
    await expect(resolveUsername(undefined as unknown as string)).rejects.toMatchObject({
      code: "INVALID_USERNAME",
    });
  });

  it("throws NOT_FOUND when the server returns 404", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ error: "User not found" }, 404)
    );
    await expect(resolveUsername("nonexistent")).rejects.toMatchObject({
      code: "NOT_FOUND",
      statusCode: 404,
    });
  });

  it("throws INVALID_USERNAME when the server returns 400", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ error: "Invalid username format" }, 400)
    );
    await expect(resolveUsername("x")).rejects.toMatchObject({
      code: "INVALID_USERNAME",
      statusCode: 400,
    });
  });

  it("throws NETWORK_ERROR for a 500 response", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ error: "Internal server error" }, 500)
    );
    await expect(resolveUsername("tolu")).rejects.toMatchObject({
      code: "NETWORK_ERROR",
      statusCode: 500,
    });
  });

  it("throws NETWORK_ERROR when fetch fails entirely", async () => {
    mockFetch.mockRejectedValueOnce(new Error("Network request failed"));
    await expect(resolveUsername("tolu")).rejects.toMatchObject({
      code: "NETWORK_ERROR",
    });
  });

  it("throws UNKNOWN_ERROR when response has no address field", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ username: "tolu" }) // missing 'address'
    );
    await expect(resolveUsername("tolu")).rejects.toMatchObject({
      code: "UNKNOWN_ERROR",
    });
  });

  it("throws UNKNOWN_ERROR when response has no username field", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ address: TEST_ADDRESS }) // missing 'username'
    );
    await expect(resolveUsername("tolu")).rejects.toMatchObject({
      code: "UNKNOWN_ERROR",
    });
  });

  it("saves the resolved username to recent recipients on success", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ username: "tolu", address: TEST_ADDRESS })
    );
    await resolveUsername("tolu");
    expect(mockAsyncStorage.setItem).toHaveBeenCalledWith(
      "recent_recipient_usernames",
      expect.stringContaining("tolu")
    );
  });
});

describe("safeResolveUsername (#911)", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockFetch.mockReset();
  });

  it("returns ReceiverDetails on success", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ username: "tolu", address: TEST_ADDRESS })
    );
    const result = await safeResolveUsername("tolu");
    expect(result).toEqual({ username: "tolu", address: TEST_ADDRESS });
  });

  it("returns null on NOT_FOUND error", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ error: "User not found" }, 404)
    );
    const result = await safeResolveUsername("nonexistent");
    expect(result).toBeNull();
  });

  it("returns null on INVALID_USERNAME error", async () => {
    const result = await safeResolveUsername("");
    expect(result).toBeNull();
  });

  it("returns null on network error", async () => {
    mockFetch.mockRejectedValueOnce(new Error("Network request failed"));
    const result = await safeResolveUsername("tolu");
    expect(result).toBeNull();
  });
});

describe("batchResolveUsernames (#911)", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockFetch.mockReset();
  });

  it("returns empty object for an empty array", async () => {
    const result = await batchResolveUsernames([]);
    expect(result).toEqual({});
  });

  it("returns empty object for a non-array input", async () => {
    const result = await batchResolveUsernames(null as unknown as string[]);
    expect(result).toEqual({});
  });

  it("resolves all usernames successfully", async () => {
    const addr1 = "GA2C5RFPE6GCKMY3US5FAB5A6T6E7Q3ABYH5O4ZN7LF3K4SKNQQQ7HZB";
    const addr2 = "GBXGQZUP5Y4YF5E5XVFG2YP2O5A6T6E7Q3ABYH5O4ZN7LF3K4SKNQQQ7";

    mockFetch
      .mockResolvedValueOnce(jsonResponse({ username: "alice", address: addr1 }))
      .mockResolvedValueOnce(jsonResponse({ username: "bob", address: addr2 }));

    const result = await batchResolveUsernames(["alice", "bob"]);
    expect(Object.keys(result)).toHaveLength(2);
    expect(result.alice).toEqual({ username: "alice", address: addr1 });
    expect(result.bob).toEqual({ username: "bob", address: addr2 });
  });

  it("handles partial failures gracefully — includes only successful lookups", async () => {
    const addr1 = "GA2C5RFPE6GCKMY3US5FAB5A6T6E7Q3ABYH5O4ZN7LF3K4SKNQQQ7HZB";

    mockFetch
      .mockResolvedValueOnce(jsonResponse({ username: "alice", address: addr1 }))
      .mockResolvedValueOnce(jsonResponse({ error: "User not found" }, 404));

    const result = await batchResolveUsernames(["alice", "ghost"]);
    expect(Object.keys(result)).toHaveLength(1);
    expect(result.alice).toEqual({ username: "alice", address: addr1 });
    expect(result.ghost).toBeUndefined();
  });

  it("lowercases all usernames in the returned map", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ username: "alice", address: TEST_ADDRESS })
    );
    const result = await batchResolveUsernames(["ALICE"]);
    expect(result.alice).toBeDefined();
  });
});

describe("resolveUsernameAsZapsUser (#911)", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockFetch.mockReset();
  });

  it("returns a ZapsUser-compatible object with username and address", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ username: "tolu", address: TEST_ADDRESS })
    );
    const result = await resolveUsernameAsZapsUser("tolu");
    expect(result.username).toBe("tolu");
    expect(result.address).toBe(TEST_ADDRESS);
    expect(result.avatar_url).toBeNull();
    expect(result.isVerified).toBe(false);
  });

  it("propagates UsernameLookupError on failure", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ error: "User not found" }, 404)
    );
    await expect(resolveUsernameAsZapsUser("ghost")).rejects.toMatchObject({
      code: "NOT_FOUND",
    });
  });
});

describe("fetchReceiverForTransaction (#911)", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockFetch.mockReset();
  });

  it("returns a validated ReceiverLookupResult on success", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ username: "tolu", address: TEST_ADDRESS })
    );

    const result = await fetchReceiverForTransaction("tolu");
    expect(result.username).toBe("tolu");
    expect(result.address).toBe(TEST_ADDRESS);
    expect(result.resolvedAt).toBeGreaterThan(0);
    expect(typeof result.resolvedAt).toBe("number");
  });

  it("validates that the resolved address is a Stellar public key", async () => {
    mockFetch.mockResolvedValueOnce(
      // Invalid address — doesn't match G[A-Z2-7]{55}
      jsonResponse({ username: "tolu", address: "NOT_A_STELLAR_KEY" })
    );

    await expect(fetchReceiverForTransaction("tolu")).rejects.toMatchObject({
      code: "UNKNOWN_ERROR",
    });
  });

  it("validates that the resolved address starts with G", async () => {
    mockFetch.mockResolvedValueOnce(
      // Address starts with X instead of G
      jsonResponse({ username: "tolu", address: "X" + "A".repeat(55) })
    );

    await expect(fetchReceiverForTransaction("tolu")).rejects.toMatchObject({
      code: "UNKNOWN_ERROR",
    });
  });

  it("validates that the resolved address is exactly 56 characters", async () => {
    mockFetch.mockResolvedValueOnce(
      // 55 chars instead of 56
      jsonResponse({ username: "tolu", address: "G" + "A".repeat(54) })
    );

    await expect(fetchReceiverForTransaction("tolu")).rejects.toMatchObject({
      code: "UNKNOWN_ERROR",
    });
  });

  it("propagates NOT_FOUND when username doesn't exist", async () => {
    mockFetch.mockResolvedValueOnce(
      jsonResponse({ error: "User not found" }, 404)
    );

    await expect(fetchReceiverForTransaction("ghost")).rejects.toMatchObject({
      code: "NOT_FOUND",
    });
  });

  it("propagates INVALID_USERNAME for empty input", async () => {
    await expect(fetchReceiverForTransaction("")).rejects.toMatchObject({
      code: "INVALID_USERNAME",
    });
  });

  it("propagates NETWORK_ERROR on fetch failure", async () => {
    mockFetch.mockRejectedValueOnce(new Error("Network request failed"));

    await expect(fetchReceiverForTransaction("tolu")).rejects.toMatchObject({
      code: "NETWORK_ERROR",
    });
  });
});
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

describe("Recent recipient local cache helper (Issue #914)", () => {
  beforeEach(async () => {
    jest.clearAllMocks();
    await clearRecentRecipientsCache();
  });

  it("stores and fetches recent recipients from local persistent storage", async () => {
    await saveRecentRecipient("alice.zaps");
    await saveRecentRecipient("bob.zaps");

    const cached = await getRecentRecipients();
    expect(cached).toEqual(["bob.zaps", "alice.zaps"]);
    expect(mockAsyncStorage.setItem).toHaveBeenCalledWith(
      "recent_recipient_usernames",
      JSON.stringify(["bob.zaps", "alice.zaps"])
    );
  });

  it("fetches from cache instantly without waiting for network", () => {
    saveRecentRecipient("instant.zaps");
    const syncResult = getCachedRecentRecipientsSync();
    expect(syncResult).toContain("instant.zaps");
  });

  it("limits cached contacts to 20 maximum", async () => {
    for (let i = 1; i <= 25; i++) {
      await saveRecentRecipient(`user${i}.zaps`);
    }

    const cached = await getRecentRecipients();
    expect(cached.length).toBe(20);
    expect(cached[0]).toBe("user25.zaps");
    expect(cached).not.toContain("user1.zaps");
  });

  it("deduplicates and moves re-used recipient to the top", async () => {
    await saveRecentRecipient("user1.zaps");
    await saveRecentRecipient("user2.zaps");
    await saveRecentRecipient("user1.zaps");

    const cached = await getRecentRecipients();
    expect(cached).toEqual(["user1.zaps", "user2.zaps"]);
  });
});
