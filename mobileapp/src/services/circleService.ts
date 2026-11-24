import { apiFetch } from "./api";
import type { CirclePreview } from "../types/circle";

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";

export class CircleServiceError extends Error {
  constructor(
    message: string,
    public readonly statusCode?: number
  ) {
    super(message);
    this.name = "CircleServiceError";
  }
}

function isCirclePreview(value: unknown): value is CirclePreview {
  if (!value || typeof value !== "object") return false;
  const circle = value as Record<string, unknown>;

  return (
    typeof circle.id === "string" &&
    typeof circle.name === "string" &&
    typeof circle.description === "string" &&
    typeof circle.objective === "string" &&
    typeof circle.goal_amount === "string" &&
    /^\d+$/.test(circle.goal_amount) &&
    typeof circle.currency === "string" &&
    typeof circle.progress_amount === "string" &&
    /^\d+$/.test(circle.progress_amount) &&
    typeof circle.contribution_amount === "string" &&
    /^\d+$/.test(circle.contribution_amount) &&
    (circle.contribution_frequency === "DAILY" ||
      circle.contribution_frequency === "WEEKLY" ||
      circle.contribution_frequency === "MONTHLY") &&
    typeof circle.contribution_rules === "string" &&
    typeof circle.membership_restrictions === "string" &&
    (circle.visibility === "PUBLIC" || circle.visibility === "PRIVATE") &&
    typeof circle.member_count === "number" &&
    Number.isInteger(circle.member_count) &&
    (circle.member_limit === null ||
      (typeof circle.member_limit === "number" &&
        Number.isInteger(circle.member_limit))) &&
    typeof circle.is_member === "boolean" &&
    typeof circle.can_join === "boolean" &&
    !(circle.visibility === "PRIVATE" && !circle.is_member) &&
    !(
      circle.can_join &&
      (circle.visibility !== "PUBLIC" || circle.is_member)
    ) &&
    (circle.join_unavailable_reason === null ||
      typeof circle.join_unavailable_reason === "string") &&
    typeof circle.owner_username === "string" &&
    typeof circle.created_at === "string"
  );
}

async function getErrorMessage(response: Response): Promise<string> {
  try {
    const body: unknown = await response.json();
    if (
      body &&
      typeof body === "object" &&
      "error" in body &&
      typeof body.error === "string"
    ) {
      return body.error;
    }
  } catch {
    // Non-JSON errors use the HTTP status below.
  }
  return `Request failed (HTTP ${response.status}).`;
}

export async function fetchCirclePreview(
  circleId: string
): Promise<CirclePreview> {
  const response = await apiFetch(
    `${API_BASE}/api/circles/${encodeURIComponent(circleId)}`
  );
  if (!response.ok) {
    throw new CircleServiceError(
      await getErrorMessage(response),
      response.status
    );
  }

  let data: unknown;
  try {
    data = await response.json();
  } catch {
    throw new CircleServiceError("The server returned an invalid response.");
  }
  if (!isCirclePreview(data)) {
    throw new CircleServiceError(
      "The server returned incomplete circle details."
    );
  }
  return data;
}

export async function joinCircle(circleId: string): Promise<void> {
  const response = await apiFetch(
    `${API_BASE}/api/circles/${encodeURIComponent(circleId)}/join`,
    { method: "POST" }
  );
  if (!response.ok) {
    throw new CircleServiceError(
      await getErrorMessage(response),
      response.status
    );
  }

  let data: unknown;
  try {
    data = await response.json();
  } catch {
    throw new CircleServiceError("The server returned an invalid response.");
  }
  if (
    !data ||
    typeof data !== "object" ||
    !("joined" in data) ||
    data.joined !== true
  ) {
    throw new CircleServiceError(
      "The server did not confirm circle membership."
    );
  }
}
