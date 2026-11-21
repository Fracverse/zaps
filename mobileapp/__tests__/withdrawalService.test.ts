import { apiFetch, fetchYieldBalance } from "../src/services/api";
import {
  submitWithdrawal,
  fetchAuthenticatedBalance,
  fetchWithdrawalHistory,
  validateWithdrawalAmount,
  WithdrawalError,
} from "../src/services/withdrawalService";

jest.mock("../src/services/api", () => ({
  apiFetch: jest.fn(),
  fetchYieldBalance: jest.fn(),
}));

const mockedApiFetch = apiFetch as jest.Mock;
const mockedBalance = fetchYieldBalance as jest.Mock;

describe("merchant withdrawal service", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("rejects invalid and over-balance amounts", () => {
    expect(() => validateWithdrawalAmount("abc", 100)).toThrow(WithdrawalError);
    expect(() => validateWithdrawalAmount("0", 100)).toThrow(WithdrawalError);
    expect(() => validateWithdrawalAmount("101", 100)).toThrow(
      expect.objectContaining({ code: "INSUFFICIENT_BALANCE" })
    );
  });

  it("returns an accepted result only for a successful backend response", async () => {
    mockedApiFetch.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        accepted: true,
        id: "withdrawal-1",
        status: "pending",
        amount: 25,
      }),
    });

    await expect(
      submitWithdrawal({
        amount: 25,
        destination: {
          bankCode: "001",
          accountName: "Test",
          accountNumberLast4: "1234",
          bankName: "Test Bank",
        },
      })
    ).resolves.toMatchObject({ accepted: true, id: "withdrawal-1" });
  });

  it("does not report success for failed or unaccepted responses", async () => {
    mockedApiFetch.mockResolvedValue({
      ok: false,
      status: 503,
      json: async () => ({ error: "Temporarily unavailable" }),
    });

    await expect(
      submitWithdrawal({
        amount: 25,
        destination: {
          bankCode: "001",
          accountName: "Test",
          accountNumberLast4: "1234",
          bankName: "Test Bank",
        },
      })
    ).rejects.toMatchObject({ code: "NETWORK_ERROR" });
  });

  it("uses authenticated yield available balance", async () => {
    mockedBalance.mockResolvedValue({ availableBalance: 42.5 });
    await expect(fetchAuthenticatedBalance()).resolves.toBe(42.5);
  });

  it("uses server withdrawal history and masks account numbers", async () => {
    mockedApiFetch.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        items: [
          {
            id: "withdrawal-1",
            amount: 25,
            currency: "NGN",
            account_number: "0123456789",
            bank_name: "Test Bank",
            status: "pending",
          },
        ],
      }),
    });

    await expect(fetchWithdrawalHistory()).resolves.toEqual([
      {
        id: "withdrawal-1",
        amount: 25,
        currency: "NGN",
        accountNumber: "•••• 6789",
        bankName: "Test Bank",
        date: "",
        status: "pending",
      },
    ]);
  });
});
