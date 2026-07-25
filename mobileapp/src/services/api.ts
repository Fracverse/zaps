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
  apy: string;
  totalYieldEarned: string;
  availableBalance: string;
  earningBalance: string;
  explanation: string;
  autoEarnEnabled: boolean;
}

export async function fetchYieldBalance(): Promise<YieldBalance> {
  const headers = await getAuthHeaders();
  try {
    const res = await fetch(`${API_BASE}/api/yield/balance`, { headers });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json() as Promise<YieldBalance>;
  } catch {
    // Fallback to mock while backend is not yet live
    return {
      apy: "8.75%",
      totalYieldEarned: "₦3,280.45",
      availableBalance: "₦32,450.00",
      earningBalance: "₦3,280.45",
      explanation:
        "Your earnings are generated from your wallet balance and may vary as rates change. APY is an annualized estimate and total yield is updated automatically over time.",
      autoEarnEnabled: true,
    };
  }
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
