"use client";
import { format } from "date-fns";
import { BridgeDeposit, BridgeDepositStatus, bridgeLogUrl } from "@/lib/api";

interface Props {
  deposits: BridgeDeposit[] | null;
  loading: boolean;
  error?: string | null;
}

const STATUS_STYLES: Record<BridgeDepositStatus, string> = {
  pending: "bg-yellow-100 text-yellow-800",
  confirmed: "bg-green-100 text-green-800",
  failed: "bg-red-100 text-red-800",
};

const STATUS_LABELS: Record<BridgeDepositStatus, string> = {
  pending: "Pending",
  confirmed: "Confirmed",
  failed: "Failed",
};

/** Transaction explorer base URLs for chains supported by Allbridge Core. */
const EXPLORERS: Record<string, string> = {
  ethereum: "https://etherscan.io/tx/",
  bsc: "https://bscscan.com/tx/",
  polygon: "https://polygonscan.com/tx/",
  arbitrum: "https://arbiscan.io/tx/",
  optimism: "https://optimistic.etherscan.io/tx/",
  base: "https://basescan.org/tx/",
  avalanche: "https://snowtrace.io/tx/",
  tron: "https://tronscan.org/#/transaction/",
  solana: "https://solscan.io/tx/",
  stellar: "https://stellar.expert/explorer/public/tx/",
};

export function explorerTxUrl(chain: string, hash: string): string | null {
  const base = EXPLORERS[chain.toLowerCase()];
  return base ? `${base}${hash}` : null;
}

export function BridgeStatusBadge({ status }: { status: BridgeDepositStatus }) {
  return (
    <span
      data-testid="bridge-status-badge"
      data-status={status}
      className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${STATUS_STYLES[status] ?? "bg-gray-100 text-gray-700"}`}
    >
      {STATUS_LABELS[status] ?? status}
    </span>
  );
}

function shortHash(hash: string) {
  return hash.length > 14 ? `${hash.slice(0, 8)}…${hash.slice(-4)}` : hash;
}

const HEADERS = ["Date", "Route", "Amount", "Bridge Tx", "Status", "Logs"];

export default function BridgeDepositMonitor({ deposits, loading, error }: Props) {
  const rows = deposits ?? [];
  const counts = rows.reduce(
    (acc, d) => ({ ...acc, [d.status]: (acc[d.status] ?? 0) + 1 }),
    {} as Partial<Record<BridgeDepositStatus, number>>,
  );

  return (
    <div>
      <div className="mb-3 flex flex-wrap items-center gap-3 text-xs text-slate-500">
        {(Object.keys(STATUS_LABELS) as BridgeDepositStatus[]).map((s) => (
          <span key={s} className="flex items-center gap-1">
            <BridgeStatusBadge status={s} />
            {counts[s] ?? 0}
          </span>
        ))}
      </div>
      {error && (
        <div className="mb-3 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">
          {error}
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
            {loading && !deposits ? (
              Array.from({ length: 3 }).map((_, i) => (
                <tr key={i}>
                  {HEADERS.map((h) => (
                    <td key={h} className="px-4 py-3">
                      <div className="h-4 animate-pulse rounded bg-slate-100" />
                    </td>
                  ))}
                </tr>
              ))
            ) : rows.length === 0 ? (
              <tr>
                <td colSpan={HEADERS.length} className="px-4 py-10 text-center text-slate-400">
                  No incoming bridge deposits
                </td>
              </tr>
            ) : (
              rows.map((d) => {
                const srcUrl = explorerTxUrl(d.source_chain, d.bridge_tx_hash);
                const dstUrl = d.target_tx_hash
                  ? explorerTxUrl(d.target_chain, d.target_tx_hash)
                  : null;
                return (
                  <tr key={d.id} data-testid="bridge-deposit-row" className="hover:bg-slate-50">
                    <td className="whitespace-nowrap px-4 py-3 text-slate-600">
                      {format(new Date(d.created_at), "MMM d, yyyy HH:mm")}
                    </td>
                    <td className="whitespace-nowrap px-4 py-3 capitalize text-slate-700">
                      {d.source_chain} → {d.target_chain}
                    </td>
                    <td className="px-4 py-3 font-medium">
                      {d.amount} {d.asset}
                    </td>
                    <td className="px-4 py-3 font-mono text-xs">
                      {srcUrl ? (
                        <a
                          href={srcUrl}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-indigo-600 hover:underline"
                        >
                          {shortHash(d.bridge_tx_hash)}
                        </a>
                      ) : (
                        <span className="text-slate-500">{shortHash(d.bridge_tx_hash)}</span>
                      )}
                      {dstUrl && (
                        <a
                          href={dstUrl}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="ml-2 text-slate-400 hover:text-indigo-600"
                        >
                          (dest)
                        </a>
                      )}
                    </td>
                    <td className="px-4 py-3">
                      <BridgeStatusBadge status={d.status} />
                    </td>
                    <td className="px-4 py-3 text-xs">
                      <a
                        href={bridgeLogUrl(d.id)}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-indigo-600 hover:underline"
                      >
                        View logs
                      </a>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
