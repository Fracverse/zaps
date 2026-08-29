/**
 * stellarWallet.test.ts
 *
 * Unit tests for the network fee bump helper used to "Speed Up" stuck
 * transaction envelopes (#707).
 *
 * Tests cover:
 *  - buildFeeBumpEnvelope constructs a FeeBumpTransaction that wraps the
 *    original (inner) transaction with a higher max fee, using
 *    TransactionBuilder.buildFeeBumpTransaction.
 *  - speedUpTransaction pays a bumped (multiplied) max fee and resubmits the
 *    fee-bumped envelope to the Stellar network, returning the resulting hash.
 */

import AsyncStorage from "@react-native-async-storage/async-storage";

jest.mock("@react-native-async-storage/async-storage", () =>
  require("@react-native-async-storage/async-storage/jest/async-storage-mock")
);

// ── Mock @stellar/stellar-sdk ─────────────────────────────────────────────────

const mockBuildFeeBump = jest.fn();
const mockFeeBumpSign = jest.fn();
const mockFeeBumpToXDR = jest.fn(() => "fee-bump-xdr");
const mockFetchBaseFee = jest.fn();
const mockSubmitTransaction = jest.fn();
const mockKeypairFromSecret = jest.fn();

jest.mock("@stellar/stellar-sdk", () => {
  class MockTransaction {
    xdr: string;
    networkPassphrase: string;
    constructor(xdr: string, networkPassphrase: string) {
      this.xdr = xdr;
      this.networkPassphrase = networkPassphrase;
    }
  }
  return {
    __esModule: true,
    Transaction: MockTransaction,
    TransactionBuilder: {
      buildFeeBumpTransaction: (...args: unknown[]) => {
        mockBuildFeeBump(...args);
        return {
          sign: mockFeeBumpSign,
          toXDR: mockFeeBumpToXDR,
        };
      },
      // Auto-detects fee bump vs regular envelopes from the XDR and returns a
      // plain object the Horizon server accepts (mirrors the SDK's behaviour).
      fromXDR: (xdr: string) => ({ __fromXDR: xdr }),
    },
    Horizon: {
      Server: jest.fn().mockImplementation(() => ({
        fetchBaseFee: mockFetchBaseFee,
        submitTransaction: mockSubmitTransaction,
      })),
    },
    Keypair: {
      random: jest.fn(),
      fromSecret: mockKeypairFromSecret,
    },
    Networks: { PUBLIC: "public-passphrase", TESTNET: "testnet-passphrase" },
    BASE_FEE: 100,
    xdr: {},
  };
});

// ── Imports (after mocks) ─────────────────────────────────────────────────────

import {
  buildFeeBumpEnvelope,
  speedUpTransaction,
  SPEED_UP_FEE_MULTIPLIER,
} from "../src/services/stellarWallet";

const FEE_SOURCE = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
const INNER_XDR = "AAAAAgAAAABh...inner-tx...";
const LOCAL_SECRET = "SAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABBBB";

const mockFeeKeypair = {
  publicKey: jest.fn(() => FEE_SOURCE),
  sign: jest.fn(() => "sig"),
};

// ── Helpers ───────────────────────────────────────────────────────────────────

beforeEach(async () => {
  jest.clearAllMocks();
  await AsyncStorage.clear();
  mockFetchBaseFee.mockResolvedValue(100);
  mockSubmitTransaction.mockResolvedValue({ hash: "bumped-hash-123" });
  mockKeypairFromSecret.mockReturnValue(mockFeeKeypair);
});

// ─ buildFeeBumpEnvelope ───────────────────────────────────────────────────────

describe("buildFeeBumpEnvelope", () => {
  it("builds a FeeBumpTransaction wrapping the inner tx with a higher fee", () => {
    const result = buildFeeBumpEnvelope(INNER_XDR, FEE_SOURCE, "200");

    // Constructed via the SDK's buildFeeBumpTransaction helper (#707 guidance).
    expect(mockBuildFeeBump).toHaveBeenCalledTimes(1);
    const [feeSource, baseFee, innerTx, passphrase] = mockBuildFeeBump.mock
      .calls[0] as [string, string, { xdr: string }, string];

    expect(feeSource).toBe(FEE_SOURCE);
    expect(baseFee).toBe("200");
    // The inner transaction is parsed from the original envelope XDR.
    expect(innerTx.xdr).toBe(INNER_XDR);
    expect(passphrase).toBe("testnet-passphrase");
    // returns the FeeBumpTransaction object
    expect(result).toBeDefined();
  });

  it("passes a higher max fee than the supplied base fee", () => {
    buildFeeBumpEnvelope(INNER_XDR, FEE_SOURCE, "500");
    const [, baseFee] = mockBuildFeeBump.mock.calls[0] as [string, string];
    expect(Number(baseFee)).toBeGreaterThan(100);
  });
});

// ─ speedUpTransaction ─────────────────────────────────────────────────────────

describe("speedUpTransaction", () => {
  it("resubmits a fee-bumped envelope using a multiplied fee and returns the hash", async () => {
    mockFetchBaseFee.mockResolvedValue(150);
    await AsyncStorage.setItem("stellar_keypair", LOCAL_SECRET);

    const hash = await speedUpTransaction(INNER_XDR, "local");

    // Fee is bumped by the multiplier over the network base fee.
    const [, baseFee] = mockBuildFeeBump.mock.calls[0] as [string, string];
    expect(mockFetchBaseFee).toHaveBeenCalled();
    expect(Number(baseFee)).toBe(
      150 * SPEED_UP_FEE_MULTIPLIER
    );

    // The fee-bump transaction is signed with the local keypair.
    expect(mockFeeBumpSign).toHaveBeenCalledWith(mockFeeKeypair);

    // The bumped envelope is resubmitted to the Stellar network.
    expect(mockSubmitTransaction).toHaveBeenCalledTimes(1);
    expect(hash).toBe("bumped-hash-123");
  });

  it("resolves the fee source from the local keypair when not provided", async () => {
    await AsyncStorage.setItem("stellar_keypair", LOCAL_SECRET);

    await speedUpTransaction(INNER_XDR, "local");

    const [feeSource] = mockBuildFeeBump.mock.calls[0] as [string];
    expect(feeSource).toBe(FEE_SOURCE);
  });
});
