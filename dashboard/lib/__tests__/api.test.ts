import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  api,
  clearSession,
  getRefreshToken,
  getToken,
  refreshAccessToken,
  storeSession,
  REFRESH_TOKEN_KEY,
  TOKEN_KEY,
} from "@/lib/api";

const BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080";

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 401 ? "Unauthorized" : "OK",
    json: async () => body,
  } as Response;
}

/** Authorization header from a recorded fetch call. */
function authHeaderOf(call: [string, RequestInit]): string | undefined {
  return (call[1].headers as Record<string, string>)?.Authorization;
}

describe("api client", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    localStorage.clear();
    fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
  });

  describe("session storage", () => {
    it("stores the access and refresh tokens together", () => {
      storeSession("access-1", "refresh-1");

      expect(getToken()).toBe("access-1");
      expect(getRefreshToken()).toBe("refresh-1");
      expect(localStorage.getItem(TOKEN_KEY)).toBe("access-1");
      expect(localStorage.getItem(REFRESH_TOKEN_KEY)).toBe("refresh-1");
    });

    it("leaves an existing refresh token alone when none is supplied", () => {
      storeSession("access-1", "refresh-1");
      storeSession("access-2");

      expect(getToken()).toBe("access-2");
      expect(getRefreshToken()).toBe("refresh-1");
    });

    it("clears both tokens", () => {
      storeSession("access-1", "refresh-1");
      clearSession();

      expect(getToken()).toBeNull();
      expect(getRefreshToken()).toBeNull();
    });
  });

  describe("authenticated requests", () => {
    it("sends the bearer token", async () => {
      storeSession("access-1", "refresh-1");
      fetchMock.mockResolvedValue(jsonResponse({ total_users: 1 }));

      await api.dashboardStats();

      expect(authHeaderOf(fetchMock.mock.calls[0])).toBe("Bearer access-1");
    });

    it("omits the Authorization header when there is no token", async () => {
      fetchMock.mockResolvedValue(jsonResponse({ total_users: 1 }));

      await api.dashboardStats();

      expect(authHeaderOf(fetchMock.mock.calls[0])).toBeUndefined();
    });

    it("throws on a non-401 error status", async () => {
      storeSession("access-1", "refresh-1");
      fetchMock.mockResolvedValue(jsonResponse({}, 500));

      await expect(api.dashboardStats()).rejects.toThrow("500");
      // No refresh should have been attempted for a server error.
      expect(fetchMock).toHaveBeenCalledTimes(1);
    });
  });

  describe("401 handling", () => {
    it("calls the refresh endpoint and retries the original request", async () => {
      storeSession("stale-access", "refresh-1");

      fetchMock
        .mockResolvedValueOnce(jsonResponse({}, 401)) // original request
        .mockResolvedValueOnce(
          jsonResponse({ token: "fresh-access", refresh_token: "refresh-2" }),
        ) // refresh
        .mockResolvedValueOnce(jsonResponse({ total_users: 7 })); // retry

      const result = await api.dashboardStats();

      expect(result).toEqual({ total_users: 7 });
      expect(fetchMock).toHaveBeenCalledTimes(3);

      // The middle call is the refresh, carrying the stored refresh token.
      const [refreshUrl, refreshInit] = fetchMock.mock.calls[1];
      expect(refreshUrl).toBe(`${BASE}/auth/refresh`);
      expect(refreshInit.method).toBe("POST");
      expect(JSON.parse(refreshInit.body as string)).toEqual({
        refresh_token: "refresh-1",
      });
    });

    it("replays the original request unchanged apart from the new token", async () => {
      storeSession("stale-access", "refresh-1");

      fetchMock
        .mockResolvedValueOnce(jsonResponse({}, 401))
        .mockResolvedValueOnce(jsonResponse({ token: "fresh-access" }))
        .mockResolvedValueOnce(jsonResponse({ qr_data: "x" }));

      await api.generateQr({ amount: "10", asset: "USDC" } as never);

      const [firstUrl, firstInit] = fetchMock.mock.calls[0];
      const [retryUrl, retryInit] = fetchMock.mock.calls[2];

      expect(retryUrl).toBe(firstUrl);
      expect(retryInit.method).toBe(firstInit.method);
      expect(retryInit.body).toBe(firstInit.body);
      expect(authHeaderOf(fetchMock.mock.calls[0])).toBe("Bearer stale-access");
      expect(authHeaderOf(fetchMock.mock.calls[2])).toBe("Bearer fresh-access");
    });

    it("stores the rotated tokens from the refresh response", async () => {
      storeSession("stale-access", "refresh-1");

      fetchMock
        .mockResolvedValueOnce(jsonResponse({}, 401))
        .mockResolvedValueOnce(
          jsonResponse({ token: "fresh-access", refresh_token: "refresh-2" }),
        )
        .mockResolvedValueOnce(jsonResponse({ total_users: 1 }));

      await api.dashboardStats();

      expect(getToken()).toBe("fresh-access");
      expect(getRefreshToken()).toBe("refresh-2");
    });

    it("does not retry when there is no refresh token to spend", async () => {
      localStorage.setItem(TOKEN_KEY, "stale-access");
      fetchMock.mockResolvedValue(jsonResponse({}, 401));

      await expect(api.dashboardStats()).rejects.toThrow("401");
      expect(fetchMock).toHaveBeenCalledTimes(1);
    });

    it("surfaces the original 401 and clears the session when refresh is rejected", async () => {
      storeSession("stale-access", "refresh-1");

      fetchMock
        .mockResolvedValueOnce(jsonResponse({}, 401))
        .mockResolvedValueOnce(jsonResponse({ error: "expired" }, 401));

      await expect(api.dashboardStats()).rejects.toThrow("401");

      expect(fetchMock).toHaveBeenCalledTimes(2);
      expect(getToken()).toBeNull();
      expect(getRefreshToken()).toBeNull();
    });

    it("retries only once, even if the replay also returns 401", async () => {
      storeSession("stale-access", "refresh-1");

      fetchMock
        .mockResolvedValueOnce(jsonResponse({}, 401))
        .mockResolvedValueOnce(jsonResponse({ token: "fresh-access" }))
        .mockResolvedValueOnce(jsonResponse({}, 401));

      await expect(api.dashboardStats()).rejects.toThrow("401");
      // Original + refresh + one replay. No loop.
      expect(fetchMock).toHaveBeenCalledTimes(3);
    });

    it("keeps the session when the refresh call fails on the network", async () => {
      storeSession("stale-access", "refresh-1");

      fetchMock
        .mockResolvedValueOnce(jsonResponse({}, 401))
        .mockRejectedValueOnce(new Error("network down"));

      await expect(api.dashboardStats()).rejects.toThrow("401");

      // A transient outage must not log the user out — a reload can retry.
      expect(getToken()).toBe("stale-access");
      expect(getRefreshToken()).toBe("refresh-1");
    });

    it("clears the session when the refresh response omits a token", async () => {
      storeSession("stale-access", "refresh-1");

      fetchMock
        .mockResolvedValueOnce(jsonResponse({}, 401))
        .mockResolvedValueOnce(jsonResponse({ expires_in: 900 }));

      await expect(api.dashboardStats()).rejects.toThrow("401");
      expect(getToken()).toBeNull();
    });
  });

  describe("concurrent 401s", () => {
    it("spends the refresh token once for several simultaneous failures", async () => {
      storeSession("stale-access", "refresh-1");

      fetchMock.mockImplementation(async (url: string, init: RequestInit) => {
        if (url === `${BASE}/auth/refresh`) {
          return jsonResponse({ token: "fresh-access", refresh_token: "refresh-2" });
        }
        // Anything still presenting the stale token gets a 401.
        const auth = (init?.headers as Record<string, string>)?.Authorization;
        return auth === "Bearer fresh-access"
          ? jsonResponse({ ok: true })
          : jsonResponse({}, 401);
      });

      await Promise.allSettled([
        api.dashboardStats(),
        api.myProfile(),
        api.yieldStats(),
      ]);

      const refreshCalls = fetchMock.mock.calls.filter(
        (c) => c[0] === `${BASE}/auth/refresh`,
      );
      // The backend rotates refresh tokens; a second exchange would invalidate
      // the first and log the user out despite a valid session.
      expect(refreshCalls).toHaveLength(1);
    });
  });

  describe("refreshAccessToken", () => {
    it("returns null when there is nothing to exchange", async () => {
      await expect(refreshAccessToken()).resolves.toBeNull();
      expect(fetchMock).not.toHaveBeenCalled();
    });

    it("returns the new access token", async () => {
      storeSession("stale", "refresh-1");
      fetchMock.mockResolvedValue(jsonResponse({ token: "fresh" }));

      await expect(refreshAccessToken()).resolves.toBe("fresh");
    });
  });
});
