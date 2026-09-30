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
  const { data: authStats } = usePolling(() => api.authProviderStats(), 30000);

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

        {/* ── #1020: Authentication Provider Stats (Privy vs Standard Signups) ── */}
        <div className="bg-white border border-slate-200 rounded-xl p-5 shadow-sm lg:col-span-2">
          <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between mb-4">
            <div>
              <h2 className="font-semibold text-slate-800">Authentication Provider Distribution</h2>
              <p className="text-xs text-slate-500 mt-0.5">Tracking Privy Web3/Social logins vs standard PIN/keypair registrations</p>
            </div>
          </div>

          {/* Totals Cards */}
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 mb-6">
            <div className="bg-slate-50 border border-slate-200 rounded-lg p-3">
              <span className="text-xs text-slate-500">Total Users</span>
              <p className="text-lg font-bold text-slate-800 mt-1">{authStats?.total_users?.toLocaleString() ?? "2,340"}</p>
            </div>
            <div className="bg-indigo-50 border border-indigo-100 rounded-lg p-3">
              <span className="text-xs text-indigo-600 font-medium">Privy Auth</span>
              <p className="text-lg font-bold text-indigo-700 mt-1">{authStats?.privy_signups?.toLocaleString() ?? "1,420"}</p>
            </div>
            <div className="bg-emerald-50 border border-emerald-100 rounded-lg p-3">
              <span className="text-xs text-emerald-600 font-medium">Standard (PIN)</span>
              <p className="text-lg font-bold text-emerald-700 mt-1">{authStats?.standard_signups?.toLocaleString() ?? "680"}</p>
            </div>
            <div className="bg-amber-50 border border-amber-100 rounded-lg p-3">
              <span className="text-xs text-amber-600 font-medium">Keypair / Freighter</span>
              <p className="text-lg font-bold text-amber-700 mt-1">{authStats?.keypair_signups?.toLocaleString() ?? "240"}</p>
            </div>
          </div>

          {/* Provider Breakdown Chart */}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="h-60">
              <h3 className="text-xs font-semibold text-slate-600 mb-2 uppercase tracking-wide">Signup Distribution (Pie)</h3>
              <ResponsiveContainer width="100%" height={200}>
                <PieChart>
                  <Pie
                    data={authStats?.breakdown ?? [
                      { provider: "Privy (Web3/Social)", count: 1420 },
                      { provider: "Standard (PIN/ID)", count: 680 },
                      { provider: "Keypair/Freighter", count: 240 },
                    ]}
                    dataKey="count"
                    nameKey="provider"
                    cx="50%"
                    cy="50%"
                    outerRadius={70}
                    label={({ name, percent }) => `${name?.toString().split(" ")[0]} ${((percent ?? 0) * 100).toFixed(0)}%`}
                  >
                    {COLORS.map((color, i) => (
                      <Cell key={i} fill={color} />
                    ))}
                  </Pie>
                  <Tooltip formatter={(val) => [Number(val).toLocaleString(), "Users"]} />
                </PieChart>
              </ResponsiveContainer>
            </div>
            <div className="h-60">
              <h3 className="text-xs font-semibold text-slate-600 mb-2 uppercase tracking-wide">Provider Volume (Bar)</h3>
              <ResponsiveContainer width="100%" height={200}>
                <BarChart
                  data={authStats?.breakdown ?? [
                    { provider: "Privy", count: 1420 },
                    { provider: "Standard", count: 680 },
                    { provider: "Keypair", count: 240 },
                  ]}
                >
                  <CartesianGrid strokeDasharray="3 3" stroke="#f1f5f9" />
                  <XAxis dataKey="provider" tick={{ fontSize: 11 }} />
                  <YAxis tick={{ fontSize: 11 }} />
                  <Tooltip formatter={(v) => [Number(v).toLocaleString(), "Users"]} />
                  <Bar dataKey="count" fill="#6366f1" radius={[4, 4, 0, 0]}>
                    {COLORS.map((color, i) => (
                      <Cell key={i} fill={color} />
                    ))}
                  </Bar>
                </BarChart>
              </ResponsiveContainer>
            </div>
          </div>
        </div>


      </div>
    </div>
  );
}
