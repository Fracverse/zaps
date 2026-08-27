"use client";
import { useMemo } from "react";
import { usePolling } from "@/lib/use-polling";
import { api } from "@/lib/api";
import {
  AreaChart, Area, BarChart, Bar, PieChart, Pie, Cell,
  XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
  LineChart, Line,
} from "recharts";
import { format, parseISO, startOfDay, startOfMonth } from "date-fns";

const COLORS = ["#6366f1", "#22c55e", "#f59e0b", "#ef4444", "#8b5cf6"];

export default function AnalyticsPage() {
  const { data: txs, loading } = usePolling(() => api.transactions(), 30000);

  const { dailyVolume, statusDist, assetDist, monthlyYield } = useMemo(() => {
    if (!txs) return { dailyVolume: [], statusDist: [], assetDist: [], monthlyYield: [] };

    const byDay: Record<string, number> = {};
    txs.forEach((t) => {
      const day = format(startOfDay(parseISO(t.created_at)), "MMM d");
      byDay[day] = (byDay[day] ?? 0) + t.send_amount / 1_000_000;
    });
    const dailyVolume = Object.entries(byDay)
      .slice(-30)
      .map(([date, volume]) => ({ date, volume: Number(volume.toFixed(2)) }));

    const bySt: Record<string, number> = {};
    txs.forEach((t) => { bySt[t.status] = (bySt[t.status] ?? 0) + 1; });
    const statusDist = Object.entries(bySt).map(([name, value]) => ({ name, value }));

    const byAsset: Record<string, number> = {};
    txs.forEach((t) => { byAsset[t.send_asset] = (byAsset[t.send_asset] ?? 0) + t.send_amount / 1_000_000; });
    const assetDist = Object.entries(byAsset).map(([name, value]) => ({ name, value: Number(value.toFixed(2)) }));

    const completed = txs.filter((t) => t.status === "completed");
    const byMonth: Record<string, number> = {};
    completed.forEach((t) => {
      const month = format(startOfMonth(parseISO(t.created_at)), "MMM yyyy");
      byMonth[month] = (byMonth[month] ?? 0) + t.send_amount / 1_000_000;
    });
    const monthlyYield = Object.entries(byMonth).map(([month, yield_]) => ({ month, yield: Number(yield_.toFixed(2)) }));

    return { dailyVolume, statusDist, assetDist, monthlyYield };
  }, [txs]);

  if (loading && !txs) {
    return (
      <div>
        <h1 className="text-2xl font-bold text-slate-900 mb-6">Analytics</h1>
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="bg-white border border-slate-200 rounded-xl p-5 h-72 animate-pulse" />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div>
      <h1 className="text-2xl font-bold text-slate-900 mb-6">Analytics</h1>
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">

        {/* Daily Volume */}
        <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm lg:col-span-2">
          <h2 className="font-semibold text-slate-800 mb-4">Daily Transaction Volume</h2>
          <ResponsiveContainer width="100%" height={240}>
            <AreaChart data={dailyVolume}>
              <defs>
                <linearGradient id="vol" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#6366f1" stopOpacity={0.2} />
                  <stop offset="95%" stopColor="#6366f1" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 3" stroke="#f1f5f9" />
              <XAxis dataKey="date" tick={{ fontSize: 11 }} />
              <YAxis tick={{ fontSize: 11 }} />
              <Tooltip formatter={(v) => [`${Number(v).toLocaleString()}`, "Volume"]} />
              <Area type="monotone" dataKey="volume" stroke="#6366f1" fill="url(#vol)" strokeWidth={2} />
            </AreaChart>
          </ResponsiveContainer>
        </div>

        {/* Status Distribution */}
        <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
          <h2 className="font-semibold text-slate-800 mb-4">Transaction Status</h2>
          <ResponsiveContainer width="100%" height={220}>
            <PieChart>
              <Pie data={statusDist} dataKey="value" nameKey="name" cx="50%" cy="50%" outerRadius={80} label={({ name, percent }) => `${name} ${((percent ?? 0) * 100).toFixed(0)}%`}>
                {statusDist.map((_, i) => <Cell key={i} fill={COLORS[i % COLORS.length]} />)}
              </Pie>
              <Tooltip />
            </PieChart>
          </ResponsiveContainer>
        </div>

        {/* Asset Distribution */}
        <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm">
          <h2 className="font-semibold text-slate-800 mb-4">Volume by Asset</h2>
          <ResponsiveContainer width="100%" height={220}>
            <BarChart data={assetDist}>
              <CartesianGrid strokeDasharray="3 3" stroke="#f1f5f9" />
              <XAxis dataKey="name" tick={{ fontSize: 11 }} />
              <YAxis tick={{ fontSize: 11 }} />
              <Tooltip formatter={(v) => [Number(v).toLocaleString(), "Volume"]} />
              <Bar dataKey="value" radius={[4, 4, 0, 0]}>
                {assetDist.map((_, i) => <Cell key={i} fill={COLORS[i % COLORS.length]} />)}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </div>

        {/* Yield Distribution over Time */}
        <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm lg:col-span-2">
          <h2 className="font-semibold text-slate-800 mb-4">Yield Distribution Over Time (Monthly)</h2>
          <ResponsiveContainer width="100%" height={240}>
            <LineChart data={monthlyYield}>
              <CartesianGrid strokeDasharray="3 3" stroke="#f1f5f9" />
              <XAxis dataKey="month" tick={{ fontSize: 11 }} />
              <YAxis tick={{ fontSize: 11 }} />
              <Tooltip formatter={(v) => [Number(v).toLocaleString(), "Yield"]} />
              <Line type="monotone" dataKey="yield" stroke="#22c55e" strokeWidth={2} dot={{ r: 4 }} activeDot={{ r: 6 }} />
            </LineChart>
          </ResponsiveContainer>
        </div>

      </div>
    </div>
  );
}
