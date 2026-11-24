interface Trend {
  /** Raw delta for the period, e.g. new registrations this week. */
  value: number;
  /** Optional percentage change to display alongside the raw delta. */
  percent?: number;
  /** Defaults to `value >= 0`. Set explicitly if the sign doesn't map to "good". */
  positive?: boolean;
}

interface Props {
  label: string;
  value: string | number;
  sub?: string;
  color?: string;
  trend?: Trend;
}

function TrendBadge({ trend }: { trend: Trend }) {
  const isPositive = trend.positive ?? trend.value >= 0;
  const sign = trend.value > 0 ? "+" : trend.value < 0 ? "" : "±";
  const pct =
    trend.percent !== undefined
      ? ` (${trend.percent > 0 ? "+" : ""}${trend.percent.toFixed(1)}%)`
      : "";

  return (
    <span
      data-testid="trend-badge"
      className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-semibold ring-1 ring-inset ${
        isPositive
          ? "bg-emerald-50 text-emerald-700 ring-emerald-600/20"
          : "bg-red-50 text-red-700 ring-red-600/20"
      }`}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="12"
        height="12"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        className={isPositive ? "" : "rotate-180"}
      >
        <polyline points="18 15 12 9 6 15" />
      </svg>
      {sign}
      {trend.value.toLocaleString()}
      {pct}
    </span>
  );
}

export default function StatCard({ label, value, sub, color = "text-slate-900", trend }: Props) {
  return (
    <div className="bg-white rounded-xl border border-slate-200 p-5 shadow-sm">
      <div className="flex items-start justify-between gap-2">
        <p className="text-xs font-medium text-slate-500 uppercase tracking-wide">{label}</p>
        {trend && <TrendBadge trend={trend} />}
      </div>
      <p className={`text-2xl font-bold mt-1 ${color}`}>{value}</p>
      {sub && <p className="text-xs text-slate-400 mt-1">{sub}</p>}
    </div>
  );
}
