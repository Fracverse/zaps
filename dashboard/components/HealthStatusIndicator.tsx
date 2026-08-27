"use client";

// #785 — Health status indicator for the dashboard navbar.
// Polls GET /health every 15 s and shows green/red badges per component.

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

/** A single coloured dot with a label. */
function StatusDot({
  label,
  ok,
  latencyMs,
}: {
  label: string;
  ok: boolean;
  latencyMs?: number;
}) {
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
        <span className="text-xs text-slate-400 dark:text-slate-500">
          {latencyMs}ms
        </span>
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
  const ref = useRef<HTMLDivElement>(null);

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
              : allOk
                ? "bg-emerald-500"
                : "bg-red-500",
          ].join(" ")}
        />
        <span className="hidden sm:inline">
          {!hasData ? "Health" : allOk ? "All systems OK" : "Degraded"}
        </span>
      </button>

      {open && (
        <div
          role="tooltip"
          data-testid="health-status-panel"
          className="absolute right-0 top-full z-50 mt-2 w-56 rounded-xl border border-slate-200 bg-white p-3 shadow-lg dark:border-slate-700 dark:bg-slate-800"
        >
          <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-400 dark:text-slate-500">
            Backend services
          </p>

          {error && !data && (
            <p className="text-xs text-red-600 dark:text-red-400">
              Could not reach /health
            </p>
          )}

          {data && (
            <ul className="flex flex-col gap-2">
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
                  />
                </li>
              ))}
            </ul>
          )}

          {data && (
            <p className="mt-3 text-xs text-slate-400 dark:text-slate-500">
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
