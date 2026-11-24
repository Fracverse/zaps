import { fetchCirclePreview, joinCircle } from "../src/services/circleService";
import { apiFetch } from "../src/services/api";

jest.mock("../src/services/api", () => ({
  apiFetch: jest.fn(),
}));

const mockApiFetch = jest.mocked(apiFetch);

function jsonResponse(body: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: jest.fn().mockResolvedValue(body),
  } as unknown as Response;
}

const circle = {
  id: "circle-1",
  name: "Community Fund",
  description: "Save together.",
  objective: "Reach the shared goal.",
  goal_amount: "10000",
  currency: "NGN",
  progress_amount: "2500",
  contribution_amount: "500",
  contribution_frequency: "WEEKLY",
  contribution_rules: "Contribute every week.",
  membership_restrictions: "Open to local residents.",
  visibility: "PUBLIC",
  member_count: 5,
  member_limit: 10,
  is_member: false,
  can_join: true,
  join_unavailable_reason: null,
  owner_username: "organizer",
  created_at: "2026-11-24 10:00:00",
};

describe("circle API service", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("fetches and validates circle details", async () => {
    mockApiFetch.mockResolvedValueOnce(jsonResponse(circle));

    await expect(fetchCirclePreview("circle/1")).resolves.toEqual(circle);
    expect(mockApiFetch).toHaveBeenCalledWith(
      expect.stringContaining("/api/circles/circle%2F1")
    );
  });

  it("surfaces access-denied and unavailable responses without circle data", async () => {
    mockApiFetch.mockResolvedValueOnce(
      jsonResponse({ error: "Circle not found or unavailable." }, 404)
    );

    await expect(fetchCirclePreview("private-circle")).rejects.toMatchObject({
      message: "Circle not found or unavailable.",
      statusCode: 404,
    });
  });

  it("rejects incomplete detail payloads instead of rendering partial information", async () => {
    mockApiFetch.mockResolvedValueOnce(jsonResponse({ id: "circle-1" }));

    await expect(fetchCirclePreview("circle-1")).rejects.toThrow(
      "The server returned incomplete circle details."
    );
  });

  it("fails closed if a private preview is not marked as a member-only response", async () => {
    mockApiFetch.mockResolvedValueOnce(
      jsonResponse({ ...circle, visibility: "PRIVATE", can_join: false })
    );

    await expect(fetchCirclePreview("private-circle")).rejects.toThrow(
      "The server returned incomplete circle details."
    );
  });

  it("confirms successful joins through the protected join endpoint", async () => {
    mockApiFetch.mockResolvedValueOnce(jsonResponse({ joined: true }));

    await expect(joinCircle("circle-1")).resolves.toBeUndefined();
    expect(mockApiFetch).toHaveBeenCalledWith(
      expect.stringContaining("/api/circles/circle-1/join"),
      { method: "POST" }
    );
  });
});
