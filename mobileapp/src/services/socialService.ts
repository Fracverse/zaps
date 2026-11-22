import { getSessionToken } from "./api";
import { fetchWithRetry } from "../utils/retry";

const API_BASE = process.env.EXPO_PUBLIC_API_URL || "http://localhost:8080";

async function authHeaders(): Promise<Record<string, string>> {
  const token = await getSessionToken();
  return {
    "Content-Type": "application/json",
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  };
}

export interface FeedComment {
  id: string;
  username: string;
  content: string;
  created_at: string;
}

export async function fetchFeed(): Promise<any[]> {
  const headers = await authHeaders();
  const res = await fetchWithRetry(`${API_BASE}/api/social/feed`, {
    headers,
  });
  const data = await res.json();
  return data;
}

export async function likePayment(paymentId: string): Promise<void> {
  const headers = await authHeaders();
  await fetchWithRetry(`${API_BASE}/api/social/like`, {
    method: "POST",
    headers,
    body: JSON.stringify({ payment_id: paymentId }),
  });
}

export async function unlikePayment(paymentId: string): Promise<void> {
  const headers = await authHeaders();
  await fetchWithRetry(`${API_BASE}/api/social/unlike`, {
    method: "DELETE",
    headers,
    body: JSON.stringify({ payment_id: paymentId }),
  });
}

/** POST /api/social/comment — persists a comment against a payment. */
export async function addComment(
  paymentId: string,
  content: string
): Promise<FeedComment> {
  const headers = await authHeaders();
  const res = await fetchWithRetry(`${API_BASE}/api/social/comment`, {
    method: "POST",
    headers,
    body: JSON.stringify({ payment_id: paymentId, content }),
  });
  return res.json();
}
