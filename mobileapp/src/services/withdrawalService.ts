import { apiFetch, fetchYieldBalance } from "./api";
import { fetchBankAccount } from "./bankAccountService";

export interface LinkedBankDestination {
  id?: string;
  bankCode?: string;
  accountName: string;
  accountNumberLast4: string;
  bankName: string;
}

export interface WithdrawalHistoryItem {
  id: string;
  amount: number;
  currency: string;
  accountNumber: string;
  bankName: string;
  date: string;
  status: "completed" | "pending" | "failed";
}

export interface WithdrawalRequest {
  amount: number;
  destination: LinkedBankDestination;
}

export interface WithdrawalResult {
  id: string;
  status: "pending" | "completed";
  accepted: true;
  amount: number;
}

export class WithdrawalError extends Error {
  constructor(
    message: string,
    public code:
      | "INVALID_AMOUNT"
      | "INVALID_DESTINATION"
      | "INSUFFICIENT_BALANCE"
      | "NO_DESTINATION"
      | "NETWORK_ERROR"
      | "HTTP_ERROR"
  ) {
    super(message);
  }
}

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";

function numberValue(value: unknown, fallback = 0): number {
  if (typeof value === "number") return value;
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) return parsed;
  }
  return fallback;
}

export function validateWithdrawalAmount(
  amount: string | number,
  balance: number
): number {
  const numeric =
    typeof amount === "number" ? amount : Number(String(amount).trim());
  if (!Number.isFinite(numeric) || numeric <= 0) {
    throw new WithdrawalError("Enter a valid withdrawal amount.", "INVALID_AMOUNT");
  }
  if (Math.abs(numeric * 100 - Math.round(numeric * 100)) > 0.00001) {
    throw new WithdrawalError("Use no more than two decimal places.", "INVALID_AMOUNT");
  }
  if (numeric > balance) {
    throw new WithdrawalError("Amount exceeds your available balance.", "INSUFFICIENT_BALANCE");
  }
  return Math.round(numeric * 100) / 100;
}

export async function fetchLinkedBankDestination(): Promise<LinkedBankDestination | null> {
  const { bank_account } = await fetchBankAccount();
  if (!bank_account) return null;
  return {
    bankCode: bank_account.bank_code,
    accountName: bank_account.account_name,
    accountNumberLast4: bank_account.account_number_last4,
    bankName: bank_account.bank_name,
  };
}

export async function fetchWithdrawalHistory(): Promise<WithdrawalHistoryItem[]> {
  const response = await apiFetch(`${API_BASE}/api/withdrawals/history`);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const data = await response.json();
  const items = Array.isArray(data) ? data : data.items ?? data.history ?? [];
  return items.map((item: any, index: number) => {
    const rawAccountNumber = String(
      item.account_number_last4 ??
        item.accountNumberLast4 ??
        item.account_number ??
        item.accountNumber ??
        item.destination?.accountNumber ??
        ""
    );
    const lastFour = rawAccountNumber.replace(/\D/g, "").slice(-4);
    return {
      id: String(item.id ?? item.request_id ?? index),
      amount: numberValue(item.amount ?? item.amount_usd),
      currency: String(item.currency ?? item.asset ?? ""),
      accountNumber: lastFour ? `•••• ${lastFour}` : "",
      bankName: String(
        item.bankName ?? item.bank_name ?? item.destination?.bankName ?? ""
      ),
      date: String(item.date ?? item.created_at ?? item.createdAt ?? ""),
      status:
        item.status === "failed"
          ? "failed"
          : item.status === "pending" || item.status === "processing"
            ? "pending"
            : "completed",
    };
  });
}

export async function fetchAuthenticatedBalance(): Promise<number> {
  const balance = await fetchYieldBalance();
  return numberValue(balance.availableBalance);
}

export async function submitWithdrawal(
  request: WithdrawalRequest
): Promise<WithdrawalResult> {
  if (
    !request.destination?.bankCode ||
    !request.destination?.accountNumberLast4 ||
    !request.destination?.bankName
  ) {
    throw new WithdrawalError("A linked destination is required.", "NO_DESTINATION");
  }
  if (!Number.isFinite(request.amount) || request.amount <= 0) {
    throw new WithdrawalError("Enter a valid withdrawal amount.", "INVALID_AMOUNT");
  }

  let response: Response;
  try {
    response = await apiFetch(
      `${API_BASE}${process.env.EXPO_PUBLIC_WITHDRAWAL_ENDPOINT ?? "/api/withdrawals"}`,
      {
        method: "POST",
        body: JSON.stringify({
          amount: request.amount,
          destination: {
            id: request.destination.id,
            bank_code: request.destination.bankCode,
            account_number_last4: request.destination.accountNumberLast4,
          },
        }),
      }
    );
  } catch {
    throw new WithdrawalError(
      "Could not reach the withdrawal service. Try again.",
      "NETWORK_ERROR"
    );
  }

  if (!response.ok) {
    let message = "Withdrawal could not be accepted.";
    try {
      const data = await response.json();
      message = data.error ?? data.message ?? message;
    } catch {}
    throw new WithdrawalError(
      message,
      response.status >= 500 || response.status === 429 ? "NETWORK_ERROR" : "HTTP_ERROR"
    );
  }

  const data = await response.json();
  const accepted =
    data.accepted === true ||
    data.status === "accepted" ||
    data.status === "pending" ||
    data.status === "completed";
  if (!accepted) {
    throw new WithdrawalError("Withdrawal was not accepted.", "HTTP_ERROR");
  }
  return {
    id: String(data.id ?? data.request_id ?? data.withdrawal_id ?? ""),
    status: data.status === "completed" ? "completed" : "pending",
    accepted: true,
    amount: numberValue(data.amount, request.amount),
  };
}

export const fetchWithdrawalBalance = fetchAuthenticatedBalance;
export const fetchLinkedDestination = fetchLinkedBankDestination;
export const fetchBankDestination = fetchLinkedBankDestination;
