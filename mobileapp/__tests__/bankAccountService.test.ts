import { apiFetch } from "../src/services/api";
import {
  fetchBankAccount,
  fetchSupportedBanks,
  saveBankAccount,
  verifyBankAccount,
} from "../src/services/bankAccountService";

jest.mock("../src/services/api", () => ({
  apiFetch: jest.fn(),
}));

const mockApiFetch = apiFetch as jest.MockedFunction<typeof apiFetch>;

function response(body: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    json: jest.fn().mockResolvedValue(body),
  } as unknown as Response;
}

describe("bank account API service", () => {
  beforeEach(() => mockApiFetch.mockReset());

  it("loads supported banks through the authenticated API", async () => {
    mockApiFetch.mockResolvedValueOnce(
      response({ banks: [{ name: "Bank", code: "001" }] })
    );
    await expect(fetchSupportedBanks()).resolves.toEqual({
      banks: [{ name: "Bank", code: "001" }],
    });
    expect(mockApiFetch).toHaveBeenCalledWith(
      expect.stringContaining("/api/bank-accounts/banks"),
      undefined
    );
  });

  it("loads the current merchant bank account", async () => {
    mockApiFetch.mockResolvedValueOnce(response({ bank_account: null }));
    await expect(fetchBankAccount()).resolves.toEqual({ bank_account: null });
  });

  it("verifies and persists the selected bank account", async () => {
    const account = { bank_code: "001", account_number: "0123456789" };
    mockApiFetch
      .mockResolvedValueOnce(response({ account_name: "Ada Example" }))
      .mockResolvedValueOnce(
        response({ bank_account: { ...account, account_name: "Ada Example" } })
      );

    await expect(verifyBankAccount(account)).resolves.toEqual({
      account_name: "Ada Example",
    });
    await expect(saveBankAccount(account)).resolves.toMatchObject({
      bank_account: { account_name: "Ada Example" },
    });
    expect(mockApiFetch).toHaveBeenNthCalledWith(
      1,
      expect.stringContaining("/api/bank-accounts/verify"),
      expect.objectContaining({ method: "POST", body: JSON.stringify(account) })
    );
    expect(mockApiFetch).toHaveBeenNthCalledWith(
      2,
      expect.stringMatching(/\/api\/bank-accounts\/$/),
      expect.objectContaining({ method: "PUT", body: JSON.stringify(account) })
    );
  });

  it("surfaces API errors to the screen", async () => {
    mockApiFetch.mockResolvedValueOnce(
      response({ error: "Account could not be verified." }, 422)
    );
    await expect(
      verifyBankAccount({ bank_code: "001", account_number: "0123456789" })
    ).rejects.toThrow("Account could not be verified.");
  });
});
