import { fetchWithRetry } from "../utils/retry";

const API_BASE = process.env.EXPO_PUBLIC_API_URL || "http://localhost:8080";

export async function likePayment(paymentId: string): Promise<void> {
  await fetchWithRetry(`${API_BASE}/api/social/like`, {
    method: "POST",
    body: JSON.stringify({ payment_id: paymentId }),
  });
}

export async function unlikePayment(paymentId: string): Promise<void> {
  await fetchWithRetry(`${API_BASE}/api/social/unlike`, {
    method: "DELETE",
    body: JSON.stringify({ payment_id: paymentId }),
  });
}
