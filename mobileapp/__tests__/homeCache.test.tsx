/**
 * Tests for issue #895 – AsyncStorage-backed feed cache in HomeScreen.
 *
 * Strategy: mount HomeScreen with all heavy dependencies mocked so the
 * only observable behaviour is the two-phase load (cache → network) and
 * the write-back after a successful fetch.
 */

// AsyncStorage mock must be registered before any module imports.
jest.mock("@react-native-async-storage/async-storage", () =>
  require("@react-native-async-storage/async-storage/jest/async-storage-mock")
);

import React from "react";
import { render, waitFor, act } from "@testing-library/react-native";
import AsyncStorage from "@react-native-async-storage/async-storage";

// ── Heavy native / third-party mocks ─────────────────────────────────────────

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

// ── Service mocks ─────────────────────────────────────────────────────────────

const mockFetchFeed = jest.fn();
const mockLikePayment = jest.fn();
const mockUnlikePayment = jest.fn();
const mockAddComment = jest.fn();

jest.mock("../../src/services/socialService", () => ({
  fetchFeed: (...args: unknown[]) => mockFetchFeed(...args),
  likePayment: (...args: unknown[]) => mockLikePayment(...args),
  unlikePayment: (...args: unknown[]) => mockUnlikePayment(...args),
  addComment: (...args: unknown[]) => mockAddComment(...args),
}));

const mockFetchYieldBalance = jest.fn();
const mockUpdateAutoEarn = jest.fn();

jest.mock("../../src/services/api", () => ({
  fetchYieldBalance: (...args: unknown[]) => mockFetchYieldBalance(...args),
  updateAutoEarn: (...args: unknown[]) => mockUpdateAutoEarn(...args),
}));

jest.mock("../../src/services/notificationService", () => ({
  shouldShowNotificationConsentPrompt: jest.fn(() => Promise.resolve(false)),
  markNotificationPromptDismissed: jest.fn(() => Promise.resolve()),
  registerForPushNotificationsAsync: jest.fn(() => Promise.resolve()),
  requestNotificationPermissionsAsync: jest.fn(() => Promise.resolve("denied")),
  saveNotificationPreference: jest.fn(() => Promise.resolve()),
}));

jest.mock("../../src/utils/balanceDisplay", () => ({
  formatBalance: (_val: number, _cur: string) => "₦0.00",
  formatFiatBalance: (_val: number) => "₦0.00",
  fiatFromTokens: (_val: number) => 0,
}));

jest.mock("../../src/constants/colors", () => ({
  COLORS: { primary: "#000", secondary: "#fff" },
}));

// ── Helpers ───────────────────────────────────────────────────────────────────

const FEED_CACHE_KEY = "feed_items_cache";

const makeFeedItem = (id: string) => ({
  id,
  sender: `Sender${id}`,
  receiver: `Receiver${id}`,
  amount: "₦1,000",
  description: `Payment ${id}`,
  timestamp: "1h ago",
  likes: 0,
  comments: 0,
  hasLiked: false,
  visibility: "PUBLIC" as const,
});

/** A default yield response that won't throw. */
const defaultYield = {
  apy: "5%",
  availableBalance: 0,
  totalYieldEarned: 0,
  explanation: "Test",
  autoEarnEnabled: true,
};

// ── Import subject under test (after all mocks are set up) ───────────────────

// eslint-disable-next-line import/first
import HomeScreen from "../app/(personal)/home";

// ── Test suite ────────────────────────────────────────────────────────────────

describe("HomeScreen – AsyncStorage feed cache (#895)", () => {
  beforeEach(async () => {
    jest.clearAllMocks();
    await AsyncStorage.clear();

    // Default: yield call resolves quickly so it doesn't affect feed tests.
    mockFetchYieldBalance.mockResolvedValue(defaultYield);
    // Default: network fetch returns empty so cached data stays visible.
    mockFetchFeed.mockResolvedValue([]);
  });

  // ── 1. Load from cache on mount ───────────────────────────────────────────

  it("displays cached feed items immediately on mount before network responds", async () => {
    const cached = [makeFeedItem("cached-1"), makeFeedItem("cached-2")];
    await AsyncStorage.setItem(FEED_CACHE_KEY, JSON.stringify(cached));

    // Network fetch never resolves during this test so only cache is visible.
    mockFetchFeed.mockReturnValue(new Promise(() => {}));

    const { getByText } = render(<HomeScreen />);

    await waitFor(() => {
      expect(getByText("Payment cached-1")).toBeTruthy();
      expect(getByText("Payment cached-2")).toBeTruthy();
    });
  });

  // ── 2. Fetch updates overwrite the cached data ────────────────────────────

  it("replaces cached items with fresh data after network fetch succeeds", async () => {
    const cached = [makeFeedItem("old-1")];
    const fresh = [makeFeedItem("new-1"), makeFeedItem("new-2")];

    await AsyncStorage.setItem(FEED_CACHE_KEY, JSON.stringify(cached));
    mockFetchFeed.mockResolvedValue(fresh);

    const { getByText, queryByText } = render(<HomeScreen />);

    await waitFor(() => {
      expect(getByText("Payment new-1")).toBeTruthy();
      expect(getByText("Payment new-2")).toBeTruthy();
    });

    // Old cached item should no longer be rendered.
    expect(queryByText("Payment old-1")).toBeNull();
  });

  // ── 3. Empty cache – falls back to INITIAL_FEED ───────────────────────────

  it("renders without crashing when AsyncStorage has no cached feed", async () => {
    // Cache is empty (cleared in beforeEach); network returns nothing.
    mockFetchFeed.mockResolvedValue([]);

    const { queryByText } = render(<HomeScreen />);

    // The component should still mount and the initial static feed is shown.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 100));
    });

    // No crash: the component tree should be present.
    // INITIAL_FEED has item with description "Lunch 🍕".
    await waitFor(() => {
      expect(queryByText("Lunch 🍕")).toBeTruthy();
    });
  });

  // ── 4. AsyncStorage read error is silently swallowed ─────────────────────

  it("falls back to INITIAL_FEED when AsyncStorage.getItem throws", async () => {
    const getItemSpy = jest
      .spyOn(AsyncStorage, "getItem")
      .mockRejectedValueOnce(new Error("Storage quota exceeded"));

    mockFetchFeed.mockResolvedValue([]);

    const { queryByText } = render(<HomeScreen />);

    await act(async () => {
      await new Promise((r) => setTimeout(r, 150));
    });

    // Component still renders the initial feed instead of crashing.
    await waitFor(() => {
      expect(queryByText("Lunch 🍕")).toBeTruthy();
    });

    getItemSpy.mockRestore();
  });

  // ── 5. Fresh data is written back to AsyncStorage after successful fetch ──

  it("writes fresh feed items to AsyncStorage after a successful network fetch", async () => {
    const fresh = [makeFeedItem("fresh-1"), makeFeedItem("fresh-2")];
    mockFetchFeed.mockResolvedValue(fresh);

    render(<HomeScreen />);

    await waitFor(async () => {
      const stored = await AsyncStorage.getItem(FEED_CACHE_KEY);
      expect(stored).not.toBeNull();
      const parsed = JSON.parse(stored!);
      expect(parsed).toHaveLength(2);
      expect(parsed[0].id).toBe("fresh-1");
      expect(parsed[1].id).toBe("fresh-2");
    });
  });

  // ── 6. Network error leaves cache intact on screen ────────────────────────

  it("keeps cached items visible when the network fetch throws", async () => {
    const cached = [makeFeedItem("cached-safe")];
    await AsyncStorage.setItem(FEED_CACHE_KEY, JSON.stringify(cached));

    mockFetchFeed.mockRejectedValue(new Error("Network error"));

    const { getByText } = render(<HomeScreen />);

    await waitFor(() => {
      expect(getByText("Payment cached-safe")).toBeTruthy();
    });

    // Cache should remain unchanged after the failed fetch.
    const stored = await AsyncStorage.getItem(FEED_CACHE_KEY);
    expect(JSON.parse(stored!)[0].id).toBe("cached-safe");
  });

  // ── 7. Empty network response does not overwrite valid cache ──────────────

  it("does not overwrite AsyncStorage when the network returns an empty array", async () => {
    const cached = [makeFeedItem("keep-me")];
    await AsyncStorage.setItem(FEED_CACHE_KEY, JSON.stringify(cached));

    // Network returns empty – should not clear the stored cache.
    mockFetchFeed.mockResolvedValue([]);

    render(<HomeScreen />);

    await act(async () => {
      await new Promise((r) => setTimeout(r, 150));
    });

    const stored = await AsyncStorage.getItem(FEED_CACHE_KEY);
    expect(JSON.parse(stored!)[0].id).toBe("keep-me");
  });
});
