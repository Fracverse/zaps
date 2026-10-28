"use client";
import { useMemo } from "react";
import { format } from "date-fns";
import { AdminAuditLog } from "@/lib/api";

interface Props {
  logs: AdminAuditLog[] | null;
  loading: boolean;
  error?: string | null;
}

const ACTION_STYLES: Record<string, string> = {
  config_change: "bg-blue-100 text-blue-800",
  emergency_pause: "bg-red-100 text-red-800",
  emergency_unpause: "bg-green-100 text-green-800",
  role_change: "bg-purple-100 text-purple-800",
};

const HEADERS = ["Action", "Administrator", "IP Address", "Changed Parameters", "Timestamp"];

/** Returns a copy of `logs` ordered most recent first. */
export function sortLogsDesc(logs: AdminAuditLog[]): AdminAuditLog[] {
  return [...logs].sort(
    (a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime(),
  );
}

function fmtChanges(changes?: Record<string, unknown>): string {
  if (!changes || Object.keys(changes).length === 0) return "—";
  return Object.entries(changes)
    .map(([key, value]) => {
      if (value && typeof value === "object" && "from" in value && "to" in value) {
        const { from, to } = value as { from: unknown; to: unknown };
        return `${key}: ${String(from)} → ${String(to)}`;
      }
      return `${key}: ${typeof value === "object" ? JSON.stringify(value) : String(value)}`;
    })
    .join(", ");
}

export default function AuditLogTable({ logs, loading, error }: Props) {
  const sorted = useMemo(() => sortLogsDesc(logs ?? []), [logs]);

  return (
    <div>
      {error && (
        <div className="mb-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          {error} — showing the most recently loaded entries
        </div>
      )}
      <div className="overflow-x-auto rounded-xl border border-slate-200 bg-white shadow-sm">
        <table className="w-full text-sm">
          <thead className="border-b border-slate-200 bg-slate-50">
            <tr>
              {HEADERS.map((h) => (
                <th
                  key={h}
                  className="px-4 py-3 text-left text-xs font-semibold uppercase tracking-wide text-slate-500"
                >
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {loading && !logs ? (
              Array.from({ length: 4 }).map((_, i) => (
                <tr key={i} data-testid="audit-log-skeleton">
                  {HEADERS.map((h) => (
                    <td key={h} className="px-4 py-3">
                      <div className="h-4 animate-pulse rounded bg-slate-100" />
                    </td>
                  ))}
                </tr>
              ))
            ) : sorted.length === 0 ? (
              <tr>
                <td colSpan={HEADERS.length} className="px-4 py-10 text-center text-slate-400">
                  No administrative activity recorded yet
                </td>
              </tr>
            ) : (
              sorted.map((log) => (
                <tr key={log.id} data-testid="audit-log-row" className="hover:bg-slate-50">
                  <td className="px-4 py-3">
                    <span
                      className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${ACTION_STYLES[log.action] ?? "bg-gray-100 text-gray-700"}`}
                    >
                      {log.action.replace(/_/g, " ")}
                    </span>
                  </td>
                  <td className="px-4 py-3">
                    <div className="text-slate-800">{log.admin_email ?? log.admin_id}</div>
                    {log.admin_email && (
                      <div className="font-mono text-xs text-slate-400">{log.admin_id}</div>
                    )}
                  </td>
                  <td className="px-4 py-3 font-mono text-xs text-slate-600">{log.ip_address}</td>
                  <td className="max-w-xs px-4 py-3 text-xs text-slate-600">
                    {fmtChanges(log.changes)}
                  </td>
                  <td className="whitespace-nowrap px-4 py-3 text-slate-600">
                    <time dateTime={log.timestamp}>
                      {format(new Date(log.timestamp), "MMM d, yyyy HH:mm:ss")}
                    </time>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
