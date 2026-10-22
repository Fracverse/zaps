/**
 * Haptic feedback on the yield deposit flow (#907).
 */

import React from "react";
import { act, fireEvent, render, waitFor } from "@testing-library/react-native";
import * as Haptics from "expo-haptics";
import YieldTransaction from "../app/yield-transaction";

jest.mock("expo-haptics", () => ({
  impactAsync: jest.fn(() => Promise.resolve()),
  ImpactFeedbackStyle: { Light: "light", Medium: "medium", Heavy: "heavy" },
}));

jest.mock("expo-router", () => ({
  useRouter: () => ({ push: jest.fn(), back: jest.fn() }),
  Stack: { Screen: () => null },
}));

jest.mock("react-native-safe-area-context", () => ({
  SafeAreaView: ({ children }: { children: React.ReactNode }) => children,
  useSafeAreaInsets: () => ({ top: 0, bottom: 0, left: 0, right: 0 }),
}));

jest.mock("@expo/vector-icons", () => ({
  Ionicons: () => null,
}));

// The screen imports `../hooks/useBiometric`, which does not resolve from
// app/ (the hook lives in the repo-root src/hooks), so it has to be mocked
// virtually for the screen to load under test.
jest.mock(
  "../hooks/useBiometric",
  () => ({
    useBiometric: () => ({ enabled: false, authenticate: jest.fn() }),
  }),
  { virtual: true }
);

jest.mock("../src/services/api", () => ({
  fetchYieldBalance: jest.fn(() => new Promise(() => undefined)),
  apiFetch: jest.fn(() => Promise.resolve({ ok: true })),
}));

const impactAsync = Haptics.impactAsync as jest.Mock;

describe("YieldTransaction haptics (#907)", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("fires a light haptic when switching between deposit and withdraw", () => {
    const { getByText } = render(<YieldTransaction />);

    fireEvent.press(getByText("Withdraw"));

    expect(impactAsync).toHaveBeenCalledWith("light");
    expect(impactAsync).toHaveBeenCalledTimes(1);
  });

  it("fires a light haptic when a deposit succeeds", async () => {
    const onConfirm = jest.fn(() => Promise.resolve());
    const { getByText } = render(
      <YieldTransaction onConfirm={onConfirm} initialAvailableBalance={500} />
    );

    fireEvent.press(getByText("1"));
    fireEvent.press(getByText("0"));
    fireEvent.press(getByText("Deposit to Yield"));
    impactAsync.mockClear();

    await act(async () => {
      fireEvent.press(getByText("Confirm"));
    });

    await waitFor(() => expect(getByText("Deposit Successful!")).toBeTruthy());
    expect(onConfirm).toHaveBeenCalledWith(10, "deposit");
    expect(impactAsync).toHaveBeenCalledTimes(1);
    expect(impactAsync).toHaveBeenCalledWith("light");
  });

  it("does not fire the deposit haptic when a withdrawal succeeds", async () => {
    const onConfirm = jest.fn(() => Promise.resolve());
    const { getByText } = render(
      <YieldTransaction
        initialMode="withdraw"
        onConfirm={onConfirm}
        initialYieldBalance={500}
      />
    );

    fireEvent.press(getByText("1"));
    fireEvent.press(getByText("0"));
    fireEvent.press(getByText("Withdraw from Yield"));
    impactAsync.mockClear();

    await act(async () => {
      fireEvent.press(getByText("Confirm"));
    });

    await waitFor(() => expect(onConfirm).toHaveBeenCalledWith(10, "withdraw"));
    expect(impactAsync).not.toHaveBeenCalled();
  });
});
