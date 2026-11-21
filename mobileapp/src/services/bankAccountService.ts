import { apiFetch } from "./api";

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";
const BANK_ACCOUNTS_URL = `${API_BASE}/api/bank-accounts`;

export interface SupportedBank {
  name: string;
  code: string;
}

export interface BankAccount {
  bank_name: string;
  bank_code: string;
  account_name: string;
  account_number_last4: string;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await apiFetch(`${BANK_ACCOUNTS_URL}${path}`, init);
  const body = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(
      typeof body?.error === "string"
        ? body.error
        : "Unable to process bank account request."
    );
  }
  return body as T;
}

export function fetchSupportedBanks(): Promise<{ banks: SupportedBank[] }> {
  return request("/banks");
}

export function fetchBankAccount(): Promise<{
  bank_account: BankAccount | null;
}> {
  return request("/");
}

export function verifyBankAccount(input: {
  bank_code: string;
  account_number: string;
}): Promise<{ account_name: string }> {
  return request("/verify", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export function saveBankAccount(input: {
  bank_code: string;
  account_number: string;
}): Promise<{ bank_account: BankAccount }> {
  return request("/", {
    method: "PUT",
    body: JSON.stringify(input),
  });
}
