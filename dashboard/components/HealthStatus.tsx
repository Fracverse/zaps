"use client";
import { useEffect, useState } from "react";
import useSWR from "swr";

type HealthStatus = {
  rpc: "healthy" | "unhealthy";
  db: "healthy" | "unhealthy";
};

const fetcher = async (url: string): Promise<HealthStatus> => {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error("Failed to fetch health status");
  }
  const data = await res.json();
  return {
    rpc: data.rpc === "healthy" ? "healthy" : "unhealthy",
    db: data.db === "healthy" ? "healthy" : "unhealthy",
  };
};

export default function HealthStatus() {
  const { data, error, isLoading } = useSWR<HealthStatus>(
    "/api/health",
    fetcher,
    {
      refreshInterval: 15000,
      revalidateOnFocus: false,
      revalidateOnReconnect: true,
    }
  );

  const [status, setStatus] = useState<HealthStatus>({
    rpc: "healthy",
    db: "healthy",
  });

  useEffect(() => {
    if (data) {
      setStatus(data);
    }
  }, [data]);

  const getStatusColor = (status: string) => {
    return status === "healthy" ? "bg-emerald-400" : "bg-rose-500";
  };

  const getStatusLabel = (status: string) => {
    return status === "healthy" ? "●" : "●";
  };

  if (isLoading && !data) {
    return (
      <div className="px-3 py-3 border-t border-slate-700">
        <div className="text-xs text-slate-400 mb-2">System Health</div>
        <div className="space-y-1.5">
          <div className="flex items-center justify-between">
            <span className="text-xs text-slate-300">RPC</span>
            <span className="text-xs text-slate-400">Loading...</span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-xs text-slate-300">Database</span>
            <span className="text-xs text-slate-400">Loading...</span>
          </div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="px-3 py-3 border-t border-slate-700">
        <div className="text-xs text-slate-400 mb-2">System Health</div>
        <div className="flex items-center justify-between">
          <span className="text-xs text-rose-400">⚠️ Unavailable</span>
        </div>
      </div>
    );
  }

  return (
    <div className="px-3 py-3 border-t border-slate-700">
      <div className="flex items-center justify-between mb-2">
        <span className="text-xs text-slate-400">System Health</span>
        <span className="text-[10px] text-slate-500">15s</span>
      </div>
      <div className="space-y-1.5">
        <div className="flex items-center justify-between">
          <span className="text-xs text-slate-300">RPC</span>
          <div className="flex items-center gap-1.5">
            <span className={`text-xs ${status.rpc === "healthy" ? "text-emerald-400" : "text-rose-400"}`}>
              {getStatusLabel(status.rpc)}
            </span>
            <span className={`h-1.5 w-1.5 rounded-full ${getStatusColor(status.rpc)}`}></span>
          </div>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-xs text-slate-300">Database</span>
          <div className="flex items-center gap-1.5">
            <span className={`text-xs ${status.db === "healthy" ? "text-emerald-400" : "text-rose-400"}`}>
              {getStatusLabel(status.db)}
            </span>
            <span className={`h-1.5 w-1.5 rounded-full ${getStatusColor(status.db)}`}></span>
          </div>
        </div>
      </div>
    </div>
  );
}
