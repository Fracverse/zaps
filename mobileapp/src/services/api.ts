import AsyncStorage from "@react-native-async-storage/async-storage";

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";
const TOKEN_KEY = "auth_token";

async function getAuthHeaders(): Promise<Record<string, string>> {
  const token = await AsyncStorage.getItem(TOKEN_KEY);
  return {
    "Content-Type": "application/json",
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  };
}

const RECENT_RECIPIENTS_KEY = "recent_recipient_usernames";
const MAX_RECENT_RECIPIENTS = 20;

export async function getRecentRecipients(): Promise<string[]> {
  try {
    const raw = await AsyncStorage.getItem(RECENT_RECIPIENTS_KEY);
    return raw ? (JSON.parse(raw) as string[]) : [];
  } catch {
    return [];
  }
}

export async function saveRecentRecipient(username: string): Promise<void> {
  if (!username) return;
  try {
    const raw = await AsyncStorage.getItem(RECENT_RECIPIENTS_KEY);
    const current = raw ? (JSON.parse(raw) as string[]) : [];
    const next = [username, ...current.filter((item) => item !== username)].slice(
      0,
      MAX_RECENT_RECIPIENTS
    );
    await AsyncStorage.setItem(RECENT_RECIPIENTS_KEY, JSON.stringify(next));
  } catch {
    // Ignore cache failures.
  }
}

export interface YieldBalance {
  apy: string | number;
  totalYieldEarned: string | number;
  availableBalance: string | number;
  earningBalance: string | number;
  explanation: string;
  autoEarnEnabled: boolean;
}

export async function fetchYieldBalance(): Promise<YieldBalance> {
  const headers = await getAuthHeaders();
  const res = await fetch(`${API_BASE}/api/yield/balance`, { headers });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json() as Promise<YieldBalance>;
}

export async function updateAutoEarn(enabled: boolean): Promise<void> {
  const headers = await getAuthHeaders();
  try {
    await fetch(`${API_BASE}/api/yield/auto-earn`, {
      method: "PATCH",
      headers,
      body: JSON.stringify({ enabled }),
    });
  } catch {
    // Non-fatal — local state already reflects the toggle
  }
}
