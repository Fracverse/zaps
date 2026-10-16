"use client";

// #785 — Health status indicator for the dashboard navbar.
// Polls GET /health every 15 s and shows green/red badges per component.
// Enhanced with historical tracking and latency alerts.

import { api, type HealthResponse } from "@/lib/api";
import { usePolling } from "@/lib/use-polling";
import { useState, useRef, useEffect } from "react";

const COMPONENT_LABELS: Record<
  keyof HealthResponse["components"],
  string
> = {
  database: "DB",
  yield_db: "Yield DB",
  soroban_rpc: "RPC",
};

const LATENCY_THRESHOLDS = {
  warning: 500, // ms
  critical: 1000, // ms
};

interface HealthHistoryEntry {
  timestamp: Date;
  status: "ok" | "degraded";
  components: Record<string, { status: string; latency_ms: number }>;
}

/** A single coloured dot with a label. */
function StatusDot({
  label,
  ok,
  latencyMs,
  showAlert,
}: {
  label: string;
  ok: boolean;
  latencyMs?: number;
  showAlert?: boolean;
}) {
  const latencyColor = latencyMs
    ? latencyMs > LATENCY_THRESHOLDS.critical
      ? "text-red-600"
      : latencyMs > LATENCY_THRESHOLDS.warning
      ? "text-amber-600"
      : "text-slate-400"
    : "text-slate-400";

  return (
    <span className="flex items-center gap-1">
      <span
        aria-hidden="true"
        className={[
          "inline-block h-2 w-2 rounded-full",
          ok ? "bg-emerald-500" : "bg-red-500",
        ].join(" ")}
      />
      <span className="text-xs font-medium text-slate-600 dark:text-slate-300">
        {label}
      </span>
      {latencyMs != null && (
        <span className={`text-xs ${latencyColor} dark:text-slate-500`}>
          {latencyMs}ms
        </span>
      )}
      {showAlert && (
        <span className="text-amber-500" title="High latency">⚠</span>
      )}
    </span>
  );
}

export default function HealthStatusIndicator() {
  const { data, error } = usePolling<HealthResponse>(
    () => api.health(),
    15_000,
  );

  const [open, setOpen] = useState(false);
  const [history, setHistory] = useState<HealthHistoryEntry[]>([]);
  const ref = useRef<HTMLDivElement>(null);

  // Track health history (last 10 checks)
  useEffect(() => {
    if (data) {
      setHistory((prev) => {
        const entry: HealthHistoryEntry = {
          timestamp: new Date(),
          status: data.status,
          components: data.components,
        };
        const updated = [entry, ...prev].slice(0, 10);
        return updated;
      });
    }
  }, [data]);

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  const allOk = !error && data?.status === "ok";
  const hasData = Boolean(data);

  // Calculate uptime percentage from history
  const uptimePercentage = history.length > 0
    ? (history.filter((h) => h.status === "ok").length / history.length) * 100
    : 100;

  // Check for latency alerts
  const hasLatencyAlert = data?.components
    ? Object.values(data.components).some(
        (c) => c.latency_ms > LATENCY_THRESHOLDS.warning
      )
    : false;

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        aria-label={`Backend health: ${allOk ? "all systems operational" : "degraded"}`}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        data-testid="health-status-btn"
        className="flex items-center gap-1.5 rounded-lg border border-slate-300 bg-white px-2.5 py-2 text-xs text-slate-600 transition-colors hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700"
      >
        {/* Overall status dot */}
        <span
          aria-hidden="true"
          className={[
            "inline-block h-2 w-2 rounded-full",
            !hasData
              ? "bg-slate-300 dark:bg-slate-600"
              : hasLatencyAlert
              ? "bg-amber-500 animate-pulse"
              : allOk
                ? "bg-emerald-500"
                : "bg-red-500",
          ].join(" ")}
        />
        <span className="hidden sm:inline">
          {!hasData ? "Health" : hasLatencyAlert ? "High Latency" : allOk ? "All systems OK" : "Degraded"}
        </span>
      </button>

      {open && (
        <div
          role="tooltip"
          data-testid="health-status-panel"
          className="absolute right-0 top-full z-50 mt-2 w-72 rounded-xl border border-slate-200 bg-white p-4 shadow-lg dark:border-slate-700 dark:bg-slate-800"
        >
          <div className="flex items-center justify-between mb-3">
            <p className="text-xs font-semibold uppercase tracking-wide text-slate-400 dark:text-slate-500">
              Backend services
            </p>
            <div className="flex items-center gap-2">
              <span className="text-xs text-slate-400">
                {uptimePercentage.toFixed(0)}% uptime
              </span>
            </div>
          </div>

          {error && !data && (
            <p className="text-xs text-red-600 dark:text-red-400">
              Could not reach /health
            </p>
          )}

          {data && (
            <>
              <ul className="flex flex-col gap-2 mb-4">
                {(
                  Object.entries(data.components) as [
                    keyof HealthResponse["components"],
                    { status: string; latency_ms: number },
                  ][]
                ).map(([key, component]) => (
                  <li key={key}>
                    <StatusDot
                      label={COMPONENT_LABELS[key]}
                      ok={component.status === "ok"}
                      latencyMs={component.latency_ms}
                      showAlert={component.latency_ms > LATENCY_THRESHOLDS.warning}
                    />
                  </li>
                ))}
              </ul>

              {/* Latency warning */}
              {hasLatencyAlert && (
                <div className="mb-4 p-2 bg-amber-50 border border-amber-200 rounded-lg">
                  <p className="text-xs text-amber-800">
                    ⚠ High latency detected on one or more services
                  </p>
                </div>
              )}

              {/* Recent history sparkline */}
              {history.length > 1 && (
                <div className="mb-3">
                  <p className="text-xs text-slate-400 mb-2">Recent checks (last 10)</p>
                  <div className="flex items-end gap-1 h-8">
                    {history.slice().reverse().map((entry, i) => (
                      <div
                        key={i}
                        className={`flex-1 rounded-t-sm ${
                          entry.status === "ok" ? "bg-emerald-500" : "bg-red-500"
                        }`}
                        style={{ height: `${(entry.status === "ok" ? 100 : 30)}%` }}
                        title={`${entry.timestamp.toLocaleTimeString()}: ${entry.status}`}
                      />
                    ))}
                  </div>
                </div>
              )}
            </>
          )}

          {data && (
            <p className="text-xs text-slate-400 dark:text-slate-500">
              Checked{" "}
              {new Date(data.checked_at).toLocaleTimeString(undefined, {
                hour: "2-digit",
                minute: "2-digit",
                second: "2-digit",
              })}
              {" · "}refreshes every 15 s
            </p>
          )}
        </div>
      )}
    </div>
  );
}
