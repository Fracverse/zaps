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
