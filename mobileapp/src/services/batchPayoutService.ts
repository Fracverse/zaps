import { apiFetch } from "./api";
import type { BatchPayoutItem, BatchPayoutSummary } from "../types/batchPayout";

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";

export interface BatchPayoutDetail {
  summary: BatchPayoutSummary;
  items: BatchPayoutItem[];
  status: string;
}

interface ApiBatchDetail {
  batch: {
    id: string;
    status: string;
    currency: string;
    total_recipients: number;
    total_amount: number;
    succeeded_count: number;
    failed_count: number;
    created_at: string;
    started_at?: string | null;
    completed_at?: string | null;
  };
  recipients: {
    id: string;
    user_id?: string | null;
    destination_address?: string | null;
    amount: number;
    status: string;
    tx_hash?: string | null;
    attempt_count: number;
    created_at: string;
  }[];
}

function formatAmount(amount: number): string {
  return (amount / 1_000_000).toFixed(2);
}

function formatTimestamp(timestamp: string): string {
  return timestamp.includes("T")
    ? timestamp
    : `${timestamp.replace(" ", "T")}Z`;
}

function recipientStatus(status: string): BatchPayoutItem["status"] {
  switch (status.toUpperCase()) {
    case "FAILED":
      return "failed";
    case "PENDING":
    case "SUBMITTED":
      return "pending";
    default:
      return "completed";
  }
}

/** Fetch and normalize a batch using the authenticated batch-detail endpoint. */
export async function fetchBatchPayoutDetail(
  id: string
): Promise<BatchPayoutDetail | null> {
  const response = await apiFetch(
    `${API_BASE}/api/payouts/batch/${encodeURIComponent(id)}`
  );
  if ([400, 404].includes(response.status)) return null;
  if (!response.ok) throw new Error(`HTTP ${response.status}`);

  const { batch, recipients } = (await response.json()) as ApiBatchDetail;
  return {
    status: batch.status,
    summary: {
      id: batch.id,
      totalAmount: formatAmount(batch.total_amount),
      currency: batch.currency,
      itemCount: batch.total_recipients,
      completedCount: batch.succeeded_count,
      failedCount: batch.failed_count,
      createdAt: formatTimestamp(batch.created_at),
    },
    items: recipients.map((recipient) => ({
      id: recipient.id,
      recipientName:
        recipient.user_id ?? recipient.destination_address ?? "Recipient",
      recipientAddress:
        recipient.destination_address ?? recipient.user_id ?? "",
      amount: formatAmount(recipient.amount),
      currency: batch.currency,
      status: recipientStatus(recipient.status),
    })),
  };
}
