import React from "react";
import { fireEvent, render, waitFor } from "@testing-library/react-native";
import BankAccountScreen from "../app/merchant/bank-account";
import {
  fetchBankAccount,
  fetchSupportedBanks,
  saveBankAccount,
  verifyBankAccount,
} from "../src/services/bankAccountService";

jest.mock("expo-router", () => ({
  useRouter: () => ({ back: jest.fn() }),
}));

jest.mock("../src/services/bankAccountService", () => ({
  fetchBankAccount: jest.fn(),
  fetchSupportedBanks: jest.fn(),
  saveBankAccount: jest.fn(),
  verifyBankAccount: jest.fn(),
}));

const mockFetchBankAccount = fetchBankAccount as jest.MockedFunction<
  typeof fetchBankAccount
>;
const mockFetchSupportedBanks = fetchSupportedBanks as jest.MockedFunction<
  typeof fetchSupportedBanks
>;
const mockSaveBankAccount = saveBankAccount as jest.MockedFunction<
  typeof saveBankAccount
>;
const mockVerifyBankAccount = verifyBankAccount as jest.MockedFunction<
  typeof verifyBankAccount
>;

const banks = [{ name: "Example Bank", code: "001" }];

describe("merchant bank account screen", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockFetchSupportedBanks.mockResolvedValue({ banks });
    mockFetchBankAccount.mockResolvedValue({ bank_account: null });
  });

  it("requires a bank and a valid account number before verification", async () => {
    const screen = render(<BankAccountScreen />);
    await screen.findByText("Verify account");

    expect(
      screen.getByRole("button", { name: "Verify account" }).props
        .accessibilityState?.disabled
    ).toBe(true);
    fireEvent.press(screen.getByLabelText("Bank: Select bank"));
    fireEvent.press(
      await screen.findByRole("button", { name: "Example Bank" })
    );
    expect(
      screen.getByRole("button", { name: "Verify account" }).props
        .accessibilityState?.disabled
    ).toBe(true);

    fireEvent.changeText(
      screen.getByTestId("bank-account-number"),
      "0123456789"
    );
    expect(
      screen.getByRole("button", { name: "Verify account" }).props
        .accessibilityState?.disabled
    ).toBe(false);
  });

  it("verifies with the provider and links the verified account", async () => {
    const account = {
      bank_name: "Example Bank",
      bank_code: "001",
      account_name: "Ada Example",
      account_number_last4: "6789",
    };
    mockVerifyBankAccount.mockResolvedValue({ account_name: "Ada Example" });
    mockSaveBankAccount.mockResolvedValue({ bank_account: account });

    const screen = render(<BankAccountScreen />);
    await screen.findByText("Verify account");
    fireEvent.press(screen.getByLabelText("Bank: Select bank"));
    fireEvent.press(
      await screen.findByRole("button", { name: "Example Bank" })
    );
    fireEvent.changeText(
      screen.getByTestId("bank-account-number"),
      "0123456789"
    );
    fireEvent.press(screen.getByRole("button", { name: "Verify account" }));

    await waitFor(() =>
      expect(mockVerifyBankAccount).toHaveBeenCalledWith({
        bank_code: "001",
        account_number: "0123456789",
      })
    );
    await waitFor(() =>
      expect(screen.getByTestId("bank-account-name").props.value).toBe(
        "Ada Example"
      )
    );
    fireEvent.press(screen.getByRole("button", { name: "Link account" }));

    await waitFor(() => expect(mockSaveBankAccount).toHaveBeenCalled());
    expect(screen.getByText("Bank account linked")).toBeTruthy();
    expect(screen.getByText("Example Bank · ••••••6789")).toBeTruthy();
  });

  it("shows a saved account in a masked linked state", async () => {
    mockFetchBankAccount.mockResolvedValue({
      bank_account: {
        bank_name: "Example Bank",
        bank_code: "001",
        account_name: "Ada Example",
        account_number_last4: "6789",
      },
    });
    const screen = render(<BankAccountScreen />);
    expect(await screen.findByText("Bank account linked")).toBeTruthy();
    expect(screen.getByText("Example Bank · ••••••6789")).toBeTruthy();
    expect(screen.queryByText("0123456789")).toBeNull();
  });

  it("shows verification failures and keeps linking unavailable", async () => {
    mockVerifyBankAccount.mockRejectedValue(
      new Error("Account was not found.")
    );
    const screen = render(<BankAccountScreen />);
    await screen.findByText("Verify account");
    fireEvent.press(screen.getByLabelText("Bank: Select bank"));
    fireEvent.press(
      await screen.findByRole("button", { name: "Example Bank" })
    );
    fireEvent.changeText(
      screen.getByTestId("bank-account-number"),
      "0123456789"
    );
    fireEvent.press(screen.getByRole("button", { name: "Verify account" }));

    expect(await screen.findByText("Account was not found.")).toBeTruthy();
    expect(screen.queryByText("Link account")).toBeNull();
  });
});
