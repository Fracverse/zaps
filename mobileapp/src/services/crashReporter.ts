import AsyncStorage from "@react-native-async-storage/async-storage";
import NetInfo from "@react-native-community/netinfo";

const CRASH_QUEUE_KEY = "zaps_crash_reports";
const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "https://api.zaps.app";
const MAX_QUEUED_REPORTS = 20;

export interface CrashReport {
  message: string;
  stack?: string;
  componentStack?: string;
  occurredAt: string;
}

function isNetworkError(error: Error): boolean {
  return (
    error.message.includes("Network request failed") ||
    error.message.includes("Network Error")
  );
}

async function readQueue(): Promise<CrashReport[]> {
  try {
    const raw = await AsyncStorage.getItem(CRASH_QUEUE_KEY);
    return raw ? (JSON.parse(raw) as CrashReport[]) : [];
  } catch {
    return [];
  }
}

/** Fire-and-forget POST of a crash report to the backend crash endpoint. */
export async function flushCrashReports(): Promise<void> {
  const queue = await readQueue();
  if (queue.length === 0) return;

  try {
    const res = await fetch(`${API_BASE}/api/v1/crashes`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ reports: queue }),
    });
    if (res.ok) {
      await AsyncStorage.removeItem(CRASH_QUEUE_KEY);
    }
  } catch {
    // Keep the queue for the next flush attempt (e.g. on reconnect).
  }
}

/**
 * Persist a crash report and attempt to send it to the backend.
 *
 * Reports are queued in AsyncStorage first (bounded) so that crashes recorded
 * while offline are still delivered once connectivity returns.
 */
export async function reportCrash(
  error: Error,
  componentStack?: string | null | undefined
): Promise<void> {
  try {
    const queue = await readQueue();
    const next: CrashReport[] = [
      ...queue,
      {
        message: error.message,
        stack: error.stack,
        componentStack: componentStack ?? undefined,
        occurredAt: new Date().toISOString(),
      },
    ].slice(-MAX_QUEUED_REPORTS);
    await AsyncStorage.setItem(CRASH_QUEUE_KEY, JSON.stringify(next));

    const state = await NetInfo.fetch();
    if (!state.isConnected) return;

    await flushCrashReports();
  } catch {
    // Crash reporting must never throw into the render path.
  }
}

/** Convenience wrapper so callers can catch network failures transparently. */
export async function reportNetworkCrash(error: Error): Promise<void> {
  if (isNetworkError(error)) {
    await reportCrash(error);
  }
}
