/**
 * Receipt generator — builds a plain-text receipt string and saves it
 * to the device's cache directory via expo-file-system, then shares it
 * via expo-sharing.
 */
// expo-file-system v18 moved legacy helpers to a sub-path
import {
  writeAsStringAsync,
  cacheDirectory,
  EncodingType,
} from "expo-file-system/legacy";
import * as Sharing from "expo-sharing";
import { Transaction } from "../types/transaction";
import { formatDate } from "./formatting";

const STELLAR_EXPLORER = "https://stellar.expert/explorer/public/tx/";

function buildReceiptText(tx: Transaction): string {
  const line = "─".repeat(40);
  const date = formatDate(tx.timestamp, {
    year: "numeric",
    month: "long",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });

  const typeLabel =
    tx.type === "sent"
      ? "SENT"
      : tx.type === "received"
        ? "RECEIVED"
        : tx.type.toUpperCase();

  const statusLabel = tx.status.toUpperCase();

  const lines: string[] = [
    "BLINK PAYMENT RECEIPT",
    line,
    `Date        : ${date}`,
    `Type        : ${typeLabel}`,
    `Status      : ${statusLabel}`,
    line,
    `Amount      : ${tx.amount} ${tx.asset}`,
    `Value       : ${tx.fiatValue} ${tx.fiatCurrency}`,
    ...(tx.fee ? [`Fee         : ${tx.fee} ${tx.feeAsset ?? ""}`] : []),
    line,
    `${tx.type === "sent" ? "To" : "From"}${" ".repeat(tx.type === "sent" ? 10 : 8)}: ${tx.addressLabel ?? tx.address}`,
    ...(tx.addressLabel ? [`Address     : ${tx.address}`] : []),
    ...(tx.memo ? [`Memo        : ${tx.memo}`] : []),
    ...(tx.network ? [`Network     : ${tx.network}`] : []),
    line,
    `Transaction ID`,
    tx.id,
    ...(tx.stellarTxHash
      ? [
          `\nStellar Hash`,
          tx.stellarTxHash,
          `\nExplorer`,
          `${STELLAR_EXPLORER}${tx.stellarTxHash}`,
        ]
      : []),
    line,
    "Thank you for using Blink.",
  ];

  return lines.join("\n");
}

/** Save receipt as a .txt file and open the share sheet. */
export async function shareReceipt(tx: Transaction): Promise<void> {
  const text = buildReceiptText(tx);
  const filename = `blink_receipt_${tx.id}.txt`;
  const path = `${cacheDirectory}${filename}`;

  await writeAsStringAsync(path, text, {
    encoding: EncodingType.UTF8,
  });

  const canShare = await Sharing.isAvailableAsync();
  if (!canShare) {
    throw new Error("Sharing is not available on this device.");
  }

  await Sharing.shareAsync(path, {
    mimeType: "text/plain",
    dialogTitle: "Share Receipt",
    UTI: "public.plain-text",
  });
}

/** Returns the receipt as a plain string (for display / copy). */
export function getReceiptText(tx: Transaction): string {
  return buildReceiptText(tx);
}
