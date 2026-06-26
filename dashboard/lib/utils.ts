import { Transaction } from "@/lib/api";

/** Convert transactions array to CSV string */
export function toCSV(rows: Transaction[]): string {
  const headers = ["id", "created_at", "from_address", "send_asset", "send_amount", "receive_amount", "status", "memo"];
  const lines = [
    headers.join(","),
    ...rows.map((r) =>
      [r.id, r.created_at, r.from_address, r.send_asset, r.send_amount, r.receive_amount ?? "", r.status, r.memo ?? ""]
        .map((v) => `"${String(v).replace(/"/g, '""')}"`)
        .join(",")
    ),
  ];
  return lines.join("\n");
}

export function downloadBlob(content: string, filename: string, mime: string) {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export function fmtAmount(amount: number, asset: string) {
  const val = amount / 1_000_000; // stroops → units
  return `${val.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 6 })} ${asset}`;
}

export function statusColor(status: string) {
  return {
    completed: "bg-green-100 text-green-800",
    pending: "bg-yellow-100 text-yellow-800",
    processing: "bg-blue-100 text-blue-800",
    failed: "bg-red-100 text-red-800",
    refunded: "bg-gray-100 text-gray-700",
  }[status] ?? "bg-gray-100 text-gray-700";
}
