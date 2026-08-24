"use client";

import { useEffect, useMemo, useState } from "react";
import { format } from "date-fns";
import * as StellarSdk from "@stellar/stellar-sdk";
import { api, type SocialFeedItem } from "@/lib/api";

interface Props {
  username?: string;
  xdr?: string;
  title?: string;
  onClose: () => void;
}

type XdrDecodeResult = {
  type: string;
  data: unknown;
  summary?: {
    source?: string;
    fee?: string;
    sequence?: string;
    memo?: string;
    operations?: { type: string; details: string }[];
    network?: string;
  };
};

const DECODE_STRATEGIES: {
  type: string;
  tryDecode: (x: string) => unknown;
}[] = [
  {
    type: "TransactionEnvelope",
    tryDecode: (x) =>
      StellarSdk.xdr.TransactionEnvelope.fromXDR(x, "base64"),
  },
  {
    type: "Transaction",
    tryDecode: (x) => StellarSdk.xdr.Transaction.fromXDR(x, "base64"),
  },
  {
    type: "TransactionResult",
    tryDecode: (x) => StellarSdk.xdr.TransactionResult.fromXDR(x, "base64"),
  },
  {
    type: "TransactionMeta",
    tryDecode: (x) => StellarSdk.xdr.TransactionMeta.fromXDR(x, "base64"),
  },
  {
    type: "SorobanTransactionData",
    tryDecode: (x) =>
      StellarSdk.xdr.SorobanTransactionData.fromXDR(x, "base64"),
  },
  {
    type: "LedgerEntry",
    tryDecode: (x) => StellarSdk.xdr.LedgerEntry.fromXDR(x, "base64"),
  },
  {
    type: "LedgerKey",
    tryDecode: (x) => StellarSdk.xdr.LedgerKey.fromXDR(x, "base64"),
  },
  {
    type: "ScVal",
    tryDecode: (x) => StellarSdk.xdr.ScVal.fromXDR(x, "base64"),
  },
  {
    type: "Operation",
    tryDecode: (x) => StellarSdk.xdr.Operation.fromXDR(x, "base64"),
  },
];

const NETWORK_PASSPHRASES = [
  { name: "TESTNET", passphrase: StellarSdk.Networks.TESTNET },
  { name: "PUBLIC", passphrase: StellarSdk.Networks.PUBLIC },
];

function bufferToDisplay(buf: Uint8Array): string {
  if (buf.length === 0) return "";
  try {
    const str = new TextDecoder("utf-8", { fatal: true }).decode(buf);
    if (/^[\x20-\x7E\n\r\t]+$/.test(str)) return str;
  } catch {
    /* not utf-8 */
  }
  const hex = Buffer.from(buf).toString("hex");
  if (hex.length <= 64) return "0x" + hex;
  return "0x" + hex.slice(0, 32) + "…" + hex.slice(-16) + ` (${buf.length} bytes)`;
}

function scValToHuman(val: StellarSdk.xdr.ScVal): string | null {
  try {
    switch (val.switch().name) {
      case "scvBool":
        return String(val.b());
      case "scvVoid":
        return "void";
      case "scvError": {
        const e = val.error();
        return `error(${e.code().name}: ${e.msg()?.toString() ?? ""})`;
      }
      case "scvU32":
        return String(val.u32());
      case "scvI32":
        return String(val.i32());
      case "scvU64":
        return val.u64().toString();
      case "scvI64":
        return val.i64().toString();
      case "scvTimepoint":
        return `timepoint(${val.timepoint().toString()})`;
      case "scvDuration":
        return `duration(${val.duration().toString()})`;
      case "scvU128":
      case "scvI128":
      case "scvU256":
      case "scvI256": {
        const hiLo = val.u128 ? val.u128() : val.i128 ? val.i128() : val.u256 ? val.u256() : val.i256()!;
        const hi = BigInt(hiLo.hi().toString());
        const lo = BigInt(hiLo.lo().toString());
        return String((hi << 64n) | lo);
      }
      case "scvBytes": {
        const b = val.bytes();
        return bufferToDisplay(new Uint8Array(b));
      }
      case "scvString":
        return val.str().toString();
      case "scvSymbol":
        return "Symbol(" + val.sym().toString() + ")";
      case "scvVec":
        return `vec(${val.vec()?.length ?? 0} items)`;
      case "scvMap":
        return `map(${val.map()?.length ?? 0} entries)`;
      case "scvAddress": {
        const addr = val.address();
        if (addr.switch().name === "scAddressTypeAccount") {
          return StellarSdk.StrKey.encodeEd25519PublicKey(
            addr.accountId().ed25519(),
          );
        }
        return StellarSdk.StrKey.encodeContract(addr.contractId());
      }
      case "scvLedgerKeyContractInstance":
        return "LedgerKeyContractInstance";
      case "scvLedgerKeyNonce":
        return "LedgerKeyNonce";
      case "scvContractInstance":
        return "ContractInstance";
      default:
        return null;
    }
  } catch {
    return null;
  }
}

function xdrToJson(value: unknown, depth = 0, maxDepth = 12): unknown {
  if (depth > maxDepth) return "[Max depth exceeded]";
  if (value === null || value === undefined) return null;

  if (
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean" ||
    typeof value === "bigint"
  ) {
    return value;
  }

  if (value instanceof Uint8Array || Buffer.isBuffer(value)) {
    return bufferToDisplay(new Uint8Array(value));
  }

  if (Array.isArray(value)) {
    return value.map((item) => xdrToJson(item, depth + 1, maxDepth));
  }

  if (typeof value === "object") {
    const obj = value as Record<string, any>;

    // Long / BN - style numbers
    if (
      typeof obj.high === "number" &&
      typeof obj.low === "number" &&
      typeof obj.unsigned === "boolean"
    ) {
      const hi = BigInt(obj.high);
      const lo = BigInt(obj.low) >>> 0n;
      const combined = (hi << 32n) | lo;
      return combined.toString();
    }

    // Handle xdr.ScVal specially
    if (value instanceof StellarSdk.xdr.ScVal) {
      const human = scValToHuman(value as StellarSdk.xdr.ScVal);
      if (human !== null) return { _type: "ScVal", _value: human };
    }

    const ctorName = (value as any).constructor?.name;
    const isXdrType =
      typeof ctorName === "string" &&
      (ctorName.startsWith("xdr$") ||
        ctorName.startsWith("Xdr") ||
        typeof (value as any).toXDR === "function");

    if (!isXdrType) {
      // Plain object, recurse entries
      const out: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
        out[k] = xdrToJson(v, depth + 1, maxDepth);
      }
      return out;
    }

    const result: Record<string, unknown> = {};
    if (ctorName) result._type = ctorName.replace(/^xdr\$/, "");

    // Union (discriminated)
    if (typeof obj.switch === "function") {
      try {
        const sw = obj.switch();
        result._switch = typeof sw?.name === "string" ? sw.name : xdrToJson(sw, depth + 1, maxDepth);
        const armVal = obj.arm?.();
        const armType = obj.armType?.();
        if (armVal !== undefined && armVal !== null && armType) {
          if (typeof armType === "string") {
            result[armType] = xdrToJson(armVal, depth + 1, maxDepth);
          } else {
            result._value = xdrToJson(armVal, depth + 1, maxDepth);
          }
        }
        return result;
      } catch {
        /* fall through to attr walk */
      }
    }

    // Struct: iterate _attributes or _fields
    const attrs =
      (obj._attributes && typeof obj._attributes === "object"
        ? Object.keys(obj._attributes)
        : null) ??
      (obj._fields && typeof obj._fields === "object"
        ? Object.keys(obj._fields)
        : null);

    if (attrs && attrs.length > 0) {
      for (const key of attrs) {
        if (typeof obj[key] === "function") {
          try {
            result[key] = xdrToJson(obj[key](), depth + 1, maxDepth);
          } catch (e) {
            result[key] = `[decode error: ${(e as Error).message}]`;
          }
        }
      }
      return result;
    }

    // Fallback: toXDR or toString
    if (typeof obj.toXDR === "function") {
      try {
        return obj.toXDR("base64");
      } catch {
        /* pass */
      }
    }
    if (typeof obj.toString === "function") {
      try {
        const s = obj.toString();
        if (s !== "[object Object]") return s;
      } catch {
        /* pass */
      }
    }
    return result;
  }

  return String(value);
}

function tryDecodeAsTransaction(xdr: string): XdrDecodeResult["summary"] {
  for (const { passphrase, name } of NETWORK_PASSPHRASES) {
    try {
      const tx = StellarSdk.TransactionBuilder.fromXDR(xdr, passphrase);
      const ops = (tx.operations || []).map((op) => ({
        type: op.type,
        details: summarizeOperation(op),
      }));
      return {
        source: tx.source,
        fee: String(tx.fee),
        sequence: String(tx.sequence),
        memo: summarizeMemo(tx.memo),
        operations: ops,
        network: name,
      };
    } catch {
      /* try next network */
    }
  }
  return undefined;
}

function summarizeMemo(memo: StellarSdk.Memo): string {
  switch (memo.type) {
    case StellarSdk.MemoText:
      return `text(${memo.value as string})`;
    case StellarSdk.MemoId:
      return `id(${memo.value as string})`;
    case StellarSdk.MemoHash:
      return `hash(0x${memo.value as string})`;
    case StellarSdk.MemoReturn:
      return `return(0x${memo.value as string})`;
    case StellarSdk.MemoNone:
    default:
      return "none";
  }
}

function summarizeOperation(op: StellarSdk.Operation): string {
  try {
    switch (op.type) {
      case "payment": {
        const p = op as StellarSdk.Operation.Payment;
        return `${p.destination} ← ${p.amount} ${p.asset.code}${p.asset.issuer ? ":" + p.asset.issuer : ""}`;
      }
      case "createAccount": {
        const c = op as StellarSdk.Operation.CreateAccount;
        return `${c.destination} starting balance ${c.startingBalance} XLM`;
      }
      case "invokeHostFunction": {
        const f = op as StellarSdk.Operation.InvokeHostFunction;
        const func = f.func;
        if (func instanceof StellarSdk.xdr.HostFunction) {
          const sw = func.switch().name;
          return `hostFn: ${sw}`;
        }
        return "invokeHostFunction";
      }
      case "extendFootprintTtl":
        return `extend TTL by ${(op as any).extendTo}`;
      case "restoreFootprint":
        return "restore footprint";
      case "bumpSequence":
        return `bump to ${(op as StellarSdk.Operation.BumpSequence).bumpTo}`;
      case "setOptions":
        return "set options";
      case "changeTrust": {
        const c = op as StellarSdk.Operation.ChangeTrust;
        return `trust ${c.line.code}${c.line.issuer ? ":" + c.line.issuer : ""} (limit ${c.limit})`;
      }
      case "allowTrust":
        return "allow trust";
      default:
        return op.type;
    }
  } catch {
    return op.type ?? "unknown";
  }
}

function decodeXdr(xdr: string): XdrDecodeResult {
  let lastErr: unknown = null;
  for (const strategy of DECODE_STRATEGIES) {
    try {
      const decoded = strategy.tryDecode(xdr);
      const summary =
        strategy.type === "TransactionEnvelope"
          ? tryDecodeAsTransaction(xdr)
          : undefined;
      return {
        type: strategy.type,
        data: xdrToJson(decoded),
        summary,
      };
    } catch (e) {
      lastErr = e;
      continue;
    }
  }
  throw lastErr instanceof Error ? lastErr : new Error("Unknown XDR format");
}

type JsonNodeProps = {
  data: unknown;
  name?: string;
  level?: number;
};

function JsonNode({ data, name, level = 0 }: JsonNodeProps) {
  const [expanded, setExpanded] = useState(level < 2);
  const depthIndent = `${level * 16}px`;

  if (data === null || data === undefined) {
    return (
      <div style={{ paddingLeft: depthIndent }} className="py-0.5 font-mono text-xs">
        {name !== undefined && (
          <span className="text-rose-600 mr-1">{name}:</span>
        )}
        <span className="text-slate-400">null</span>
      </div>
    );
  }

  if (typeof data === "string") {
    return (
      <div style={{ paddingLeft: depthIndent }} className="py-0.5 font-mono text-xs">
        {name !== undefined && (
          <span className="text-rose-600 mr-1">{name}:</span>
        )}
        <span className="text-emerald-700">"{data}"</span>
      </div>
    );
  }

  if (typeof data === "number" || typeof data === "bigint") {
    return (
      <div style={{ paddingLeft: depthIndent }} className="py-0.5 font-mono text-xs">
        {name !== undefined && (
          <span className="text-rose-600 mr-1">{name}:</span>
        )}
        <span className="text-blue-600">{String(data)}</span>
      </div>
    );
  }

  if (typeof data === "boolean") {
    return (
      <div style={{ paddingLeft: depthIndent }} className="py-0.5 font-mono text-xs">
        {name !== undefined && (
          <span className="text-rose-600 mr-1">{name}:</span>
        )}
        <span className="text-purple-600">{String(data)}</span>
      </div>
    );
  }

  if (Array.isArray(data)) {
    if (data.length === 0) {
      return (
        <div style={{ paddingLeft: depthIndent }} className="py-0.5 font-mono text-xs">
          {name !== undefined && (
            <span className="text-rose-600 mr-1">{name}:</span>
          )}
          <span className="text-slate-400">[] (empty)</span>
        </div>
      );
    }
    return (
      <div>
        <div
          style={{ paddingLeft: depthIndent }}
          className="py-0.5 font-mono text-xs cursor-pointer select-none hover:bg-slate-50 rounded"
          onClick={() => setExpanded((e) => !e)}
        >
          <span className="text-slate-500 mr-1">
            {expanded ? "▾" : "▸"}
          </span>
          {name !== undefined && (
            <span className="text-rose-600 mr-1">{name}:</span>
          )}
          <span className="text-slate-500">
            Array({data.length})
          </span>
        </div>
        {expanded && (
          <div className="border-l border-slate-200 ml-2" style={{ marginLeft: `${level * 16 + 8}px` }}>
            {data.map((item, idx) => (
              <JsonNode
                key={idx}
                data={item}
                name={`[${idx}]`}
                level={level + 1}
              />
            ))}
          </div>
        )}
      </div>
    );
  }

  if (typeof data === "object") {
    const entries = Object.entries(data as Record<string, unknown>);
    if (entries.length === 0) {
      return (
        <div style={{ paddingLeft: depthIndent }} className="py-0.5 font-mono text-xs">
          {name !== undefined && (
            <span className="text-rose-600 mr-1">{name}:</span>
          )}
          <span className="text-slate-400">{"{}"} (empty)</span>
        </div>
      );
    }
    return (
      <div>
        <div
          style={{ paddingLeft: depthIndent }}
          className="py-0.5 font-mono text-xs cursor-pointer select-none hover:bg-slate-50 rounded"
          onClick={() => setExpanded((e) => !e)}
        >
          <span className="text-slate-500 mr-1">
            {expanded ? "▾" : "▸"}
          </span>
          {name !== undefined && (
            <span className="text-rose-600 mr-1">{name}:</span>
          )}
          <span className="text-slate-600 font-semibold">
            {(data as any)._type
              ? String((data as any)._type)
              : "Object"}
          </span>
          <span className="text-slate-400 ml-1">
            {entries.length} field{entries.length === 1 ? "" : "s"}
          </span>
        </div>
        {expanded && (
          <div className="border-l border-slate-200" style={{ marginLeft: `${level * 16 + 8}px` }}>
            {entries.map(([k, v]) => {
              if (k === "_type") return null;
              return <JsonNode key={k} data={v} name={k} level={level + 1} />;
            })}
          </div>
        )}
      </div>
    );
  }

  return (
    <div style={{ paddingLeft: depthIndent }} className="py-0.5 font-mono text-xs">
      {name !== undefined && (
        <span className="text-rose-600 mr-1">{name}:</span>
      )}
      <span className="text-slate-600">{String(data)}</span>
    </div>
  );
}

function XdrViewer({ xdr }: { xdr: string }) {
  const [copied, setCopied] = useState<"raw" | "json" | null>(null);
  const decoded = useMemo(() => decodeXdr(xdr), [xdr]);
  const jsonString = useMemo(
    () => JSON.stringify(decoded.data, null, 2),
    [decoded],
  );

  const copy = async (which: "raw" | "json") => {
    const text = which === "raw" ? xdr : jsonString;
    await navigator.clipboard.writeText(text);
    setCopied(which);
    setTimeout(() => setCopied(null), 1500);
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2 rounded-lg border border-slate-200 bg-slate-50 p-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="inline-flex rounded-full bg-indigo-100 px-2 py-0.5 text-xs font-semibold text-indigo-800">
              {decoded.type}
            </span>
            {decoded.summary?.network && (
              <span className="inline-flex rounded-full bg-slate-200 px-2 py-0.5 text-xs font-medium text-slate-700">
                {decoded.summary.network}
              </span>
            )}
          </div>
          <button
            onClick={() => copy("raw")}
            className="rounded-md border border-slate-300 bg-white px-2.5 py-1 text-xs font-medium text-slate-700 hover:bg-slate-100 transition"
          >
            {copied === "raw" ? "✓ Copied" : "Copy XDR"}
          </button>
        </div>
        <pre className="max-h-24 overflow-x-auto break-all rounded bg-white p-2 font-mono text-[11px] leading-relaxed text-slate-600 ring-1 ring-slate-200">
          {xdr}
        </pre>
      </div>

      {decoded.summary && (
        <div className="rounded-lg border border-indigo-100 bg-indigo-50/40 p-3">
          <h4 className="mb-2 text-xs font-semibold uppercase tracking-wide text-indigo-700">
            Transaction Summary
          </h4>
          <dl className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-xs">
            <div>
              <dt className="font-medium text-slate-500">Source Account</dt>
              <dd className="mt-0.5 break-all font-mono text-slate-800">
                {decoded.summary.source ?? "—"}
              </dd>
            </div>
            <div>
              <dt className="font-medium text-slate-500">Sequence</dt>
              <dd className="mt-0.5 font-mono text-slate-800">
                {decoded.summary.sequence ?? "—"}
              </dd>
            </div>
            <div>
              <dt className="font-medium text-slate-500">Fee (stroops)</dt>
              <dd className="mt-0.5 font-mono text-slate-800">
                {decoded.summary.fee ?? "—"}
              </dd>
            </div>
            <div>
              <dt className="font-medium text-slate-500">Memo</dt>
              <dd className="mt-0.5 font-mono text-slate-800">
                {decoded.summary.memo ?? "—"}
              </dd>
            </div>
          </dl>
          {decoded.summary.operations && decoded.summary.operations.length > 0 && (
            <div className="mt-3">
              <dt className="font-medium text-slate-500 text-xs mb-1.5">
                Operations ({decoded.summary.operations.length})
              </dt>
              <ul className="space-y-1.5">
                {decoded.summary.operations.map((op, idx) => (
                  <li
                    key={idx}
                    className="rounded-md bg-white ring-1 ring-indigo-100 px-2.5 py-1.5 text-xs"
                  >
                    <span className="inline-flex rounded bg-indigo-100 text-indigo-800 px-1.5 py-0.5 font-semibold mr-2">
                      {op.type}
                    </span>
                    <span className="font-mono text-slate-700 break-all">
                      {op.details}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}

      <div className="rounded-lg border border-slate-200 bg-white">
        <div className="flex items-center justify-between border-b border-slate-200 px-3 py-2">
          <h4 className="text-xs font-semibold uppercase tracking-wide text-slate-600">
            Decoded Structure
          </h4>
          <button
            onClick={() => copy("json")}
            className="rounded-md border border-slate-300 bg-white px-2.5 py-1 text-xs font-medium text-slate-700 hover:bg-slate-50 transition"
          >
            {copied === "json" ? "✓ Copied" : "Copy JSON"}
          </button>
        </div>
        <div className="max-h-[55vh] overflow-auto p-3">
          <JsonNode data={decoded.data} />
        </div>
      </div>
    </div>
  );
}

export default function PaymentDetailDialog({
  username,
  xdr,
  title,
  onClose,
}: Props) {
  const [payments, setPayments] = useState<SocialFeedItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedXdr, setSelectedXdr] = useState<string | null>(xdr ?? null);
  const [decodeError, setDecodeError] = useState<string | null>(null);

  useEffect(() => {
    setSelectedXdr(xdr ?? null);
    setDecodeError(null);
  }, [xdr]);

  useEffect(() => {
    if (!username) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    api
      .userPayments(username)
      .then((data) => {
        if (!cancelled) setPayments(data);
      })
      .catch((e) => {
        if (!cancelled)
          setError(e instanceof Error ? e.message : "Failed to load");
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [username]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (selectedXdr && !xdr) {
          setSelectedXdr(null);
        } else {
          onClose();
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose, selectedXdr, xdr]);

  if (selectedXdr) {
    let content: React.ReactNode;
    try {
      content = <XdrViewer xdr={selectedXdr} />;
    } catch (e) {
      content = (
        <div className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700">
          <p className="font-semibold mb-1">Failed to decode XDR</p>
          <p className="font-mono text-xs break-all">
            {e instanceof Error ? e.message : "Unknown error"}
          </p>
        </div>
      );
    }

    const headerTitle =
      title ??
      (xdr
        ? "Transaction Details"
        : "Decoded XDR Payload");

    return (
      <div
        className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
        onClick={(e) => e.target === e.currentTarget && onClose()}
      >
        <div className="relative max-h-[90vh] w-full max-w-3xl overflow-hidden rounded-xl bg-white shadow-2xl flex flex-col">
          <div className="flex items-center justify-between border-b border-slate-200 px-6 py-4 flex-shrink-0">
            <div>
              <h2 className="text-lg font-semibold text-slate-900">
                {headerTitle}
              </h2>
              {username && !xdr && (
                <p className="text-sm text-slate-500">@{username}</p>
              )}
            </div>
            <div className="flex items-center gap-2">
              {!xdr && (
                <button
                  onClick={() => setSelectedXdr(null)}
                  className="rounded-lg px-3 py-1.5 text-sm text-slate-600 hover:bg-slate-100"
                >
                  ← Back
                </button>
              )}
              <button
                onClick={onClose}
                className="rounded-lg p-1.5 text-slate-400 hover:bg-slate-100 hover:text-slate-600"
                aria-label="Close dialog"
              >
                ✕
              </button>
            </div>
          </div>
          <div className="overflow-y-auto px-6 py-4 flex-1">
            {content}
            {decodeError && (
              <div className="mt-4 rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700">
                {decodeError}
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div className="relative max-h-[85vh] w-full max-w-2xl overflow-hidden rounded-xl bg-white shadow-2xl flex flex-col">
        <div className="flex items-center justify-between border-b border-slate-200 px-6 py-4 flex-shrink-0">
          <div>
            <h2 className="text-lg font-semibold text-slate-900">
              {title ?? "Payment History"}
            </h2>
            {username && (
              <p className="text-sm text-slate-500">@{username}</p>
            )}
          </div>
          <button
            onClick={onClose}
            className="rounded-lg p-1.5 text-slate-400 hover:bg-slate-100 hover:text-slate-600"
            aria-label="Close dialog"
          >
            ✕
          </button>
        </div>

        <div className="overflow-y-auto flex-1">
          {loading ? (
            <div className="p-6 text-center text-sm text-slate-500">
              Loading…
            </div>
          ) : error ? (
            <div className="p-6 text-center text-sm text-red-600">{error}</div>
          ) : payments.length === 0 ? (
            <div className="p-6 text-center text-sm text-slate-400">
              No payments found
            </div>
          ) : (
            <table className="w-full text-sm">
              <thead className="bg-slate-50 text-xs uppercase text-slate-500 sticky top-0 z-10">
                <tr>
                  <th className="px-4 py-3 text-left">Date</th>
                  <th className="px-4 py-3 text-left">Sender</th>
                  <th className="px-4 py-3 text-left">Receiver</th>
                  <th className="px-4 py-3 text-right">Amount</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {payments.map((p) => (
                  <tr key={p.id} className="hover:bg-slate-50">
                    <td className="px-4 py-3 text-slate-600 whitespace-nowrap">
                      {format(new Date(p.created_at), "MMM d, yyyy HH:mm")}
                    </td>
                    <td className="px-4 py-3 font-medium text-slate-900">
                      {p.sender_username}
                    </td>
                    <td className="px-4 py-3 font-medium text-slate-900">
                      {p.receiver_username}
                    </td>
                    <td className="px-4 py-3 text-right font-semibold text-slate-900">
                      {p.amount} {p.currency}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}
