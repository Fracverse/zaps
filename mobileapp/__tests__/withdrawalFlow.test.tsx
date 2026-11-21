import React from "react";
import { fireEvent, render, waitFor } from "@testing-library/react-native";
import TransferSummaryScreen from "../app/merchant/transfer-summary";
import TransferConfirmationScreen from "../app/merchant/transfer-confirmation";
import { submitWithdrawal } from "../src/services/withdrawalService";

const mockPush = jest.fn();
const mockReplace = jest.fn();
const mockBack = jest.fn();
let mockParams: Record<string, string> = {};

jest.mock("expo-router", () => ({
  useRouter: () => ({ push: mockPush, replace: mockReplace, back: mockBack }),
  useLocalSearchParams: () => mockParams,
}));

jest.mock("expo-haptics", () => ({
  impactAsync: jest.fn(() => Promise.resolve()),
  notificationAsync: jest.fn(() => Promise.resolve()),
  ImpactFeedbackStyle: { Light: "light", Medium: "medium" },
  NotificationFeedbackType: { Success: "success", Error: "error" },
}));

jest.mock("react-native-safe-area-context", () => ({
  useSafeAreaInsets: () => ({ top: 0, bottom: 0, left: 0, right: 0 }),
}));

jest.mock("@expo/vector-icons", () => ({
  Feather: () => null,
}));

jest.mock("../src/hooks/useTheme", () => ({
  useTheme: () => ({
    theme: {
      text: "#111",
      textSecondary: "#777",
      backgroundRoot: "#fff",
      backgroundDefault: "#eee",
      border: "#ddd",
      primary: "#146c43",
    },
  }),
}));

jest.mock("../src/services/withdrawalService", () => ({
  submitWithdrawal: jest.fn(),
  WithdrawalError: class WithdrawalError extends Error {},
}));

const mockedSubmit = submitWithdrawal as jest.Mock;

describe("merchant withdrawal navigation", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockParams = {
      amount: "25",
      accountName: "Ada",
      accountNumberLast4: "6789",
      bankCode: "001",
      bankName: "Example Bank",
      destinationId: "1",
    };
  });

  it("summary advances to confirmation, never directly to success", async () => {
    const screen = render(<TransferSummaryScreen />);
    fireEvent.press(screen.getByTestId("confirm-withdrawal"));
    await waitFor(() =>
      expect(mockPush).toHaveBeenCalledWith(
        expect.stringContaining("/merchant/transfer-confirmation?")
      )
    );
    expect(mockReplace).not.toHaveBeenCalledWith(
      expect.stringContaining("/merchant/success")
    );
  });

  it("does not navigate to success when submission fails", async () => {
    mockedSubmit.mockRejectedValue(new Error("Provider unavailable"));
    const screen = render(<TransferConfirmationScreen />);
    fireEvent.press(screen.getByTestId("submit-withdrawal"));

    await waitFor(() =>
      expect(screen.getByTestId("withdrawal-submit-error")).toBeTruthy()
    );
    expect(mockReplace).not.toHaveBeenCalledWith(
      expect.stringContaining("/merchant/success")
    );
  });

  it("navigates to success only after an accepted submission", async () => {
    mockedSubmit.mockResolvedValue({
      accepted: true,
      id: "request-1",
      status: "pending",
      amount: 25,
    });
    const screen = render(<TransferConfirmationScreen />);
    fireEvent.press(screen.getByTestId("submit-withdrawal"));

    await waitFor(() =>
      expect(mockReplace).toHaveBeenCalledWith(
        expect.stringContaining("/merchant/success?")
      )
    );
  });
});
