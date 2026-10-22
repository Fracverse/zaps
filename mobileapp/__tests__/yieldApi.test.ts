import * as SecureStore from "expo-secure-store";
import { fetchYieldBalance, updateAutoEarn } from "../src/services/api";

jest.mock("expo-secure-store", () => ({
  getItemAsync: jest.fn().mockResolvedValue(null),
  setItemAsync: jest.fn().mockResolvedValue(undefined),
  deleteItemAsync: jest.fn().mockResolvedValue(undefined),
}));

const mockFetch = jest.fn();
global.fetch = mockFetch as unknown as typeof fetch;

function jsonResponse(body: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: jest.fn().mockResolvedValue(body),
  } as unknown as Response;
}

describe("yield API", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockFetch.mockReset();
  });

  it("normalizes the yield endpoint response from micro-units", async () => {
    jest
      .mocked(SecureStore.getItemAsync)
      .mockResolvedValue("test-session-token");
    mockFetch.mockResolvedValueOnce(
      jsonResponse({
        available_balance: 1_250_000,
        earning_balance: 4_000_000,
        accrued_interest: 25_000,
        total_earning_balance: 4_025_000,
        apy: 5.5,
        auto_earn_enabled: false,
      })
    );

    await expect(fetchYieldBalance()).resolves.toEqual({
      apy: 5.5,
      availableBalance: 1.25,
      earningBalance: 4,
      totalYieldEarned: 4.025,
      autoEarnEnabled: false,
    });
    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringContaining("/api/yield/balance"),
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: "Bearer test-session-token",
        }),
      })
    );
  });

  it("throws when the yield endpoint responds with an error", async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({}, 503));

    await expect(fetchYieldBalance()).rejects.toThrow("HTTP 503");
  });

  it("persists the Auto-Earn setting using the supported yield endpoint", async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({ auto_earn_enabled: false }));

    await expect(updateAutoEarn(false)).resolves.toBeUndefined();
    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringContaining("/api/yield/toggle-auto"),
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ enabled: false }),
      })
    );
  });

  it("throws when saving the Auto-Earn setting fails", async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({}, 500));

    await expect(updateAutoEarn(true)).rejects.toThrow("HTTP 500");
  });
});
