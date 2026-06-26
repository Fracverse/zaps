"use client";
import { useEffect, useRef, useState } from "react";

/** Polls `fetcher` every `intervalMs`. Returns latest data + loading/error state. */
export function usePolling<T>(
  fetcher: () => Promise<T>,
  intervalMs = 15000,
  deps: unknown[] = []
) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const run = async () => {
    try {
      const result = await fetcher();
      setData(result);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Error");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    run();
    timerRef.current = setInterval(run, intervalMs);
    return () => { if (timerRef.current) clearInterval(timerRef.current); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return { data, loading, error, refresh: run };
}
