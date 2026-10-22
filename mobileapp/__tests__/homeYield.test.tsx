/**
 * Home screen yield UI: split balance card (#915), loading skeletons and
 * error/retry handling (#906), and haptic feedback on auto-earn (#907).
 */

import React from "react";
import {
  act,
  configure,
  fireEvent,
  render,
  waitFor,
  within,
} from "@testing-library/react-native";
import * as Haptics from "expo-haptics";
import HomeScreen from "../app/(personal)/home";

jest.mock("@react-native-async-storage/async-storage", () =>
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  require("@react-native-async-storage/async-storage/jest/async-storage-mock")
);

jest.mock("expo-router", () => ({
  useRouter: () => ({ push: jest.fn(), back: jest.fn(), replace: jest.fn() }),
}));

jest.mock("expo-haptics", () => ({
  selectionAsync: jest.fn(() => Promise.resolve()),
  impactAsync: jest.fn(() => Promise.resolve()),
  ImpactFeedbackStyle: { Light: "light", Medium: "medium", Heavy: "heavy" },
}));

jest.mock("expo-notifications", () => ({
  PermissionStatus: { GRANTED: "granted", DENIED: "denied" },
}));

jest.mock("react-native-safe-area-context", () => ({
  SafeAreaView: ({ children }: { children: React.ReactNode }) => children,
  useSafeAreaInsets: () => ({ top: 0, bottom: 0, left: 0, right: 0 }),
}));

jest.mock("react-native-svg", () => {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const React = require("react");
  const mock = ({ children }: { children?: React.ReactNode }) =>
    React.createElement("View", null, children);
  return {
    __esModule: true,
    default: mock,
    Svg: mock,
    Defs: mock,
    LinearGradient: mock,
    Stop: mock,
    Path: mock,
    Circle: mock,
  };
});

jest.mock("@expo/vector-icons", () => ({
  Ionicons: () => null,
}));

const mockFetchYieldBalance = jest.fn();
const mockUpdateAutoEarn = jest.fn();

jest.mock("../src/services/socialService", () => ({
  fetchFeed: jest.fn(() => Promise.resolve([])),
  likePayment: jest.fn(),
  unlikePayment: jest.fn(),
  addComment: jest.fn(),
}));

jest.mock("../src/services/api", () => ({
  fetchYieldBalance: (...args: unknown[]) => mockFetchYieldBalance(...args),
  updateAutoEarn: (...args: unknown[]) => mockUpdateAutoEarn(...args),
}));

jest.mock("../src/services/notificationService", () => ({
  shouldShowNotificationConsentPrompt: jest.fn(() => Promise.resolve(false)),
  markNotificationPromptDismissed: jest.fn(() => Promise.resolve()),
  registerForPushNotificationsAsync: jest.fn(() => Promise.resolve()),
  requestNotificationPermissionsAsync: jest.fn(() => Promise.resolve("denied")),
  saveNotificationPreference: jest.fn(() => Promise.resolve()),
}));

// 2 XLM available and 0.5 XLM earned, at the fixed 1600 NGN/XLM rate.
const loadedYield = {
  apy: 5,
  availableBalance: 2,
  totalYieldEarned: 0.5,
  explanation: "Test",
  autoEarnEnabled: true,
};

const impactAsync = Haptics.impactAsync as jest.Mock;

// The first render of the large home screen is slow on a cold transform cache.
jest.setTimeout(20000);
configure({ asyncUtilTimeout: 5000 });

describe("HomeScreen balance card split (#915)", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("shows the available and earning balances as separate numeric sections", async () => {
    mockFetchYieldBalance.mockResolvedValue(loadedYield);
    const { getByTestId, getByLabelText } = render(<HomeScreen />);

    await waitFor(() =>
      expect(getByTestId("available-balance-amount")).toBeTruthy()
    );
    fireEvent.press(getByLabelText("Show balance"));

    expect(getByTestId("available-balance-amount").props.children).toBe(
      "₦3,200.00"
    );
    expect(getByTestId("earning-balance-amount").props.children).toBe(
      "₦800.00"
    );
    expect(getByTestId("available-balance-section")).toBeTruthy();
    expect(getByTestId("earning-balance-section")).toBeTruthy();
  });

  it("keeps the numeric balances masked until the user reveals them", async () => {
    mockFetchYieldBalance.mockResolvedValue(loadedYield);
    const { getByTestId } = render(<HomeScreen />);

    await waitFor(() =>
      expect(getByTestId("available-balance-amount")).toBeTruthy()
    );

    expect(getByTestId("available-balance-amount").props.children).toBe(
      "••••••"
    );
  });
});

describe("HomeScreen yield loading and error states (#906)", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  it("shows skeletons instead of amounts while the yield data loads", async () => {
    mockFetchYieldBalance.mockReturnValue(new Promise(() => undefined));
    const { getByTestId, queryByTestId } = render(<HomeScreen />);

    expect(getByTestId("available-balance-skeleton")).toBeTruthy();
    expect(getByTestId("earning-balance-skeleton")).toBeTruthy();
    expect(getByTestId("earning-card-skeleton")).toBeTruthy();
    expect(queryByTestId("available-balance-amount")).toBeNull();
    expect(queryByTestId("earning-balance-amount")).toBeNull();
  });

  it("replaces skeletons with the balances once loaded", async () => {
    mockFetchYieldBalance.mockResolvedValue(loadedYield);
    const { queryByTestId, getByTestId } = render(<HomeScreen />);

    await waitFor(() =>
      expect(queryByTestId("available-balance-skeleton")).toBeNull()
    );
    expect(queryByTestId("earning-card-skeleton")).toBeNull();
    expect(getByTestId("earning-balance-amount")).toBeTruthy();
  });

  it("shows the error with a retry on a failed request, and recovers", async () => {
    mockFetchYieldBalance
      .mockRejectedValueOnce(new Error("HTTP 503"))
      .mockResolvedValueOnce(loadedYield);
    const { findByTestId, getByTestId, queryByTestId } = render(<HomeScreen />);

    const balanceError = await findByTestId("balance-error");
    expect(
      within(balanceError).getByText(
        /The server could not complete the request\./
      )
    ).toBeTruthy();
    expect(getByTestId("earning-card-error")).toBeTruthy();

    fireEvent.press(getByTestId("balance-retry"));

    await waitFor(() => expect(queryByTestId("balance-error")).toBeNull());
    expect(mockFetchYieldBalance).toHaveBeenCalledTimes(2);
    expect(getByTestId("earning-balance-amount")).toBeTruthy();
  });

  it("surfaces a network timeout as an error with a retry", async () => {
    jest.useFakeTimers();
    mockFetchYieldBalance.mockReturnValue(new Promise(() => undefined));
    const { getByTestId } = render(<HomeScreen />);

    await act(async () => {
      jest.advanceTimersByTime(4500);
    });

    expect(
      within(getByTestId("balance-error")).getByText(/The request timed out\./)
    ).toBeTruthy();
    expect(getByTestId("balance-retry")).toBeTruthy();
    expect(getByTestId("earning-card-error")).toBeTruthy();
  });
});

describe("HomeScreen auto-earn haptics (#907)", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockUpdateAutoEarn.mockResolvedValue(undefined);
  });

  it("fires a light haptic when auto-earn is switched off", async () => {
    mockFetchYieldBalance.mockResolvedValue(loadedYield);
    const { getByLabelText, findByTestId } = render(<HomeScreen />);
    await findByTestId("earning-balance-amount");
    impactAsync.mockClear();

    await act(async () => {
      fireEvent(
        getByLabelText("Toggle Auto-Earn on Idle Funds"),
        "valueChange",
        false
      );
    });

    expect(impactAsync).toHaveBeenCalledWith("light");
    expect(mockUpdateAutoEarn).toHaveBeenCalledWith(false);
  });

  it("fires another light haptic once enabling auto-earn succeeds", async () => {
    mockFetchYieldBalance.mockResolvedValue({
      ...loadedYield,
      autoEarnEnabled: false,
    });
    const { getByLabelText, findByTestId } = render(<HomeScreen />);
    await findByTestId("earning-balance-amount");

    // Switching on opens the explainer, which confirms with its own button.
    await act(async () => {
      fireEvent(
        getByLabelText("Toggle Auto-Earn on Idle Funds"),
        "valueChange",
        true
      );
    });
    impactAsync.mockClear();

    await act(async () => {
      fireEvent.press(getByLabelText("Turn on Auto-Earn"));
    });

    expect(mockUpdateAutoEarn).toHaveBeenCalledWith(true);
    // One bump for the press, one when the preference is saved.
    expect(impactAsync).toHaveBeenCalledTimes(2);
    expect(impactAsync).toHaveBeenLastCalledWith("light");
  });

  it("does not add a success bump when auto-earn is switched off", async () => {
    mockFetchYieldBalance.mockResolvedValue(loadedYield);
    const { getByLabelText, findByTestId } = render(<HomeScreen />);
    await findByTestId("earning-balance-amount");
    impactAsync.mockClear();

    await act(async () => {
      fireEvent(
        getByLabelText("Toggle Auto-Earn on Idle Funds"),
        "valueChange",
        false
      );
    });

    expect(impactAsync).toHaveBeenCalledTimes(1);
  });
});
