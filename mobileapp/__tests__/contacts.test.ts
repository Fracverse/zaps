/**
 * contacts.test.ts
 *
 * Unit tests for the contact lookup integration service (#699).
 *
 * Tests cover:
 *  - normalizePhoneNumber: various international/local formats
 *  - hashPhoneNumber: returns null for short/garbage numbers
 *  - requestContactsPermission: maps Expo Contacts permission statuses
 *  - buildPhoneHashMap: builds correct hash→contact mapping
 *  - batchMatchContacts: sends correct payload, returns matches
 *  - fetchMatchedContacts: full pipeline (permission → contacts → match)
 */

// ── Mocks ──────────────────────────────────────────────────────────────────────

const mockRequestPermissionsAsync = jest.fn();
const mockGetContactsAsync = jest.fn();

jest.mock("expo-contacts", () => ({
  requestPermissionsAsync: () => mockRequestPermissionsAsync(),
  getContactsAsync: (opts: unknown) => mockGetContactsAsync(opts),
  Fields: {
    PhoneNumbers: "phoneNumbers",
    Name: "name",
  },
}), { virtual: true });

const mockFetch = jest.fn();
global.fetch = mockFetch as unknown as typeof fetch;

// ── Imports (after mocks) ──────────────────────────────────────────────────────

import {
  normalizePhoneNumber,
  hashPhoneNumber,
  requestContactsPermission,
  buildPhoneHashMap,
  batchMatchContacts,
  fetchMatchedContacts,
} from "../src/services/contacts";

// ── Helpers ────────────────────────────────────────────────────────────────────

beforeEach(() => {
  jest.clearAllMocks();
  mockFetch.mockReset();
});

// ─ normalizePhoneNumber ───────────────────────────────────────────────────────

describe("normalizePhoneNumber", () => {
  it("removes non-digit characters", () => {
    expect(normalizePhoneNumber("+234 801 234 5678")).toBe("2348012345678");
  });

  it("removes leading zero (Nigerian local format)", () => {
    expect(normalizePhoneNumber("0801 234 5678")).toBe("8012345678");
  });

  it("handles US format with area code", () => {
    expect(normalizePhoneNumber("+1 (555) 000-1234")).toBe("15550001234");
  });

  it("returns digits-only string unchanged when no leading zero", () => {
    expect(normalizePhoneNumber("2348012345678")).toBe("2348012345678");
  });

  it("returns empty string for an empty input", () => {
    expect(normalizePhoneNumber("")).toBe("");
  });
});

// ─ hashPhoneNumber ────────────────────────────────────────────────────────────

describe("hashPhoneNumber", () => {
  it("returns a non-empty string for a valid normalised number", async () => {
    const hash = await hashPhoneNumber("+234 801 234 5678");
    expect(typeof hash).toBe("string");
    expect(hash!.length).toBeGreaterThan(0);
  });

  it("returns null for a number shorter than 7 digits after normalisation", async () => {
    const hash = await hashPhoneNumber("123");
    expect(hash).toBeNull();
  });

  it("returns null for a string with no digits", async () => {
    const hash = await hashPhoneNumber("abc-xyz");
    expect(hash).toBeNull();
  });

  it("produces different hashes for different numbers", async () => {
    const h1 = await hashPhoneNumber("2348012345678");
    const h2 = await hashPhoneNumber("2348012345679");
    expect(h1).not.toBe(h2);
  });

  it("produces consistent hashes for the same number", async () => {
    const h1 = await hashPhoneNumber("2348012345678");
    const h2 = await hashPhoneNumber("2348012345678");
    expect(h1).toBe(h2);
  });

  it("normalises before hashing (leading zero stripped)", async () => {
    const h1 = await hashPhoneNumber("08012345678");  // local format
    const h2 = await hashPhoneNumber("8012345678");   // already stripped
    expect(h1).toBe(h2);
  });
});

// ─ requestContactsPermission ──────────────────────────────────────────────────

describe("requestContactsPermission", () => {
  it("returns granted:true when status is 'granted'", async () => {
    mockRequestPermissionsAsync.mockResolvedValue({ status: "granted" });
    const result = await requestContactsPermission();
    expect(result.granted).toBe(true);
  });

  it("returns granted:false with reason 'denied' when status is 'denied'", async () => {
    mockRequestPermissionsAsync.mockResolvedValue({ status: "denied" });
    const result = await requestContactsPermission();
    expect(result.granted).toBe(false);
    expect(result.reason).toBe("denied");
  });

  it("returns granted:false with reason 'restricted' for other statuses", async () => {
    mockRequestPermissionsAsync.mockResolvedValue({ status: "restricted" });
    const result = await requestContactsPermission();
    expect(result.granted).toBe(false);
    expect(result.reason).toBe("restricted");
  });

  it("returns granted:false with reason 'unavailable' on error", async () => {
    mockRequestPermissionsAsync.mockRejectedValue(new Error("SDK not available"));
    const result = await requestContactsPermission();
    expect(result.granted).toBe(false);
    expect(result.reason).toBe("unavailable");
  });
});

// ─ buildPhoneHashMap ──────────────────────────────────────────────────────────

describe("buildPhoneHashMap", () => {
  it("builds a map from hash to contact metadata", async () => {
    mockGetContactsAsync.mockResolvedValue({
      data: [
        {
          name: "Tolu Ade",
          phoneNumbers: [{ number: "+234 801 234 5678" }],
        },
      ],
    });

    const map = await buildPhoneHashMap();
    const hashes = Object.keys(map);
    expect(hashes).toHaveLength(1);
    expect(map[hashes[0]].contactName).toBe("Tolu Ade");
    expect(map[hashes[0]].phoneNumber).toBe("2348012345678");
  });

  it("skips contacts without phone numbers", async () => {
    mockGetContactsAsync.mockResolvedValue({
      data: [
        { name: "No Phone", phoneNumbers: [] },
        { name: "Has Phone", phoneNumbers: [{ number: "2348012345000" }] },
      ],
    });

    const map = await buildPhoneHashMap();
    expect(Object.keys(map)).toHaveLength(1);
    expect(Object.values(map)[0].contactName).toBe("Has Phone");
  });

  it("handles multiple phone numbers on one contact", async () => {
    mockGetContactsAsync.mockResolvedValue({
      data: [
        {
          name: "Multi Phone",
          phoneNumbers: [
            { number: "2348011111111" },
            { number: "2348022222222" },
          ],
        },
      ],
    });

    const map = await buildPhoneHashMap();
    // Both numbers should be in the map, each pointing to the same contact
    expect(Object.keys(map)).toHaveLength(2);
    Object.values(map).forEach((entry) => {
      expect(entry.contactName).toBe("Multi Phone");
    });
  });

  it("returns empty map when there are no contacts", async () => {
    mockGetContactsAsync.mockResolvedValue({ data: [] });
    const map = await buildPhoneHashMap();
    expect(Object.keys(map)).toHaveLength(0);
  });
});

// ─ batchMatchContacts ─────────────────────────────────────────────────────────

describe("batchMatchContacts", () => {
  it("returns empty array when no hashes are provided", async () => {
    const result = await batchMatchContacts([]);
    expect(result).toEqual([]);
    expect(mockFetch).not.toHaveBeenCalled();
  });

  it("sends hashes to the backend and returns matches", async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        matches: [
          {
            phone_hash: "hash-abc",
            username: "tolu.zaps",
            address: "GABCDEF1234567890XYZ",
          },
        ],
      }),
    });

    const result = await batchMatchContacts(["hash-abc", "hash-xyz"]);

    expect(result).toHaveLength(1);
    expect(result[0].username).toBe("tolu.zaps");
    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringContaining("/api/v1/contacts/match"),
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ phone_hashes: ["hash-abc", "hash-xyz"] }),
      })
    );
  });

  it("includes auth token in headers when provided", async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ matches: [] }),
    });

    await batchMatchContacts(["hash-001"], "my-auth-token");

    expect(mockFetch).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: "Bearer my-auth-token",
        }),
      })
    );
  });

  it("throws when the backend responds with a non-ok status", async () => {
    mockFetch.mockResolvedValueOnce({ ok: false, status: 500 });

    await expect(batchMatchContacts(["hash-fail"])).rejects.toThrow(
      "Contact match endpoint returned HTTP 500"
    );
  });
});

// ─ fetchMatchedContacts ───────────────────────────────────────────────────────

describe("fetchMatchedContacts", () => {
  it("returns empty array when permission is denied", async () => {
    mockRequestPermissionsAsync.mockResolvedValue({ status: "denied" });

    const result = await fetchMatchedContacts();

    expect(result).toEqual([]);
    expect(mockGetContactsAsync).not.toHaveBeenCalled();
  });

  it("returns empty array when the contact list is empty", async () => {
    mockRequestPermissionsAsync.mockResolvedValue({ status: "granted" });
    mockGetContactsAsync.mockResolvedValue({ data: [] });

    const result = await fetchMatchedContacts();

    expect(result).toEqual([]);
    expect(mockFetch).not.toHaveBeenCalled();
  });

  it("maps backend matches back to contact names", async () => {
    mockRequestPermissionsAsync.mockResolvedValue({ status: "granted" });
    mockGetContactsAsync.mockResolvedValue({
      data: [
        { name: "Tolu Ade", phoneNumbers: [{ number: "2348012345678" }] },
        { name: "Kemi S.", phoneNumbers: [{ number: "2347012345999" }] },
      ],
    });

    // Capture the hashes used in the batch call
    mockFetch.mockImplementationOnce(async (url: string, init: { body: string }) => {
      const body = JSON.parse(init.body) as { phone_hashes: string[] };
      const [hash1, hash2] = body.phone_hashes;
      return {
        ok: true,
        json: async () => ({
          matches: [
            { phone_hash: hash1, username: "tolu.zaps", address: "GABCD" },
            { phone_hash: hash2, username: "kemi.zaps", address: "GXYZW" },
          ],
        }),
      };
    });

    const result = await fetchMatchedContacts();

    expect(result).toHaveLength(2);
    const names = result.map((r) => r.contactName);
    expect(names).toContain("Tolu Ade");
    expect(names).toContain("Kemi S.");
    const usernames = result.map((r) => r.username);
    expect(usernames).toContain("tolu.zaps");
    expect(usernames).toContain("kemi.zaps");
  });

  it("returns only matched contacts (not all contacts)", async () => {
    mockRequestPermissionsAsync.mockResolvedValue({ status: "granted" });
    mockGetContactsAsync.mockResolvedValue({
      data: [
        { name: "Tolu Ade", phoneNumbers: [{ number: "2348012345678" }] },
        { name: "No Match Person", phoneNumbers: [{ number: "2347099999999" }] },
      ],
    });

    mockFetch.mockImplementationOnce(async (url: string, init: { body: string }) => {
      const body = JSON.parse(init.body) as { phone_hashes: string[] };
      const [hash1] = body.phone_hashes;
      return {
        ok: true,
        json: async () => ({
          matches: [{ phone_hash: hash1, username: "tolu.zaps", address: "GABCD" }],
        }),
      };
    });

    const result = await fetchMatchedContacts();

    expect(result).toHaveLength(1);
    expect(result[0].username).toBe("tolu.zaps");
  });
});
