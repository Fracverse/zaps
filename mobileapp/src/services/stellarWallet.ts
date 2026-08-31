import * as StellarSdk from "@stellar/stellar-sdk";
import { fetchWithRetry } from "../utils/retry";
import AsyncStorage from "@react-native-async-storage/async-storage";

const API_BASE = process.env.EXPO_PUBLIC_API_URL || "http://localhost:8080";
const STELLAR_NETWORK = process.env.EXPO_PUBLIC_STELLAR_NETWORK || "TESTNET";
const HORIZON_URL =
  STELLAR_NETWORK === "PUBLIC"
    ? "https://horizon.stellar.org"
    : "https://horizon-testnet.stellar.org";

const KEY_STORAGE_KEY = "stellar_keypair";

const networkPassphrase =
  STELLAR_NETWORK === "PUBLIC"
    ? StellarSdk.Networks.PUBLIC
    : StellarSdk.Networks.TESTNET;

const server = new StellarSdk.Horizon.Server(HORIZON_URL);

export interface StellarWalletState {
  publicKey: string;
  isConnected: boolean;
  source: "freighter" | "albedo" | "local" | null;
}

export async function checkFreighter(): Promise<boolean> {
  try {
    if (typeof window !== "undefined" && (window as any).freighter) {
      return true;
    }
    return false;
  } catch {
    return false;
  }
}

export async function connectFreighter(): Promise<StellarWalletState> {
  const freighter = (window as any).freighter;
  if (!freighter) {
    throw new Error("Freighter extension not found");
  }
  const publicKey = await freighter.getPublicKey();
  return { publicKey, isConnected: true, source: "freighter" };
}

export async function signWithFreighter(txXdr: string): Promise<string> {
  const freighter = (window as any).freighter;
  if (!freighter) {
    throw new Error("Freighter extension not found");
  }
  const signedXdr = await freighter.signTransaction(txXdr, {
    networkPassphrase,
  });
  return signedXdr;
}

export async function connectAlbedo(): Promise<StellarWalletState> {
  try {
    const albedo = (window as any).Albedo;
    if (!albedo) {
      const res = await fetch("https://albedo.link");
      if (!res.ok) throw new Error("Albedo not available");
    }
    const result = await (window as any).Albedo.publicKey({
      network: STELLAR_NETWORK,
    });
    return { publicKey: result.publicKey, isConnected: true, source: "albedo" };
  } catch (e) {
    throw new Error("Failed to connect Albedo: " + (e as Error).message);
  }
}

export async function signWithAlbedo(txXdr: string): Promise<string> {
  const albedo = (window as any).Albedo;
  if (!albedo) {
    throw new Error("Albedo not available");
  }
  const result = await albedo.tx({ xdr: txXdr, network: STELLAR_NETWORK });
  return result.signed_envelope_xdr;
}

export function generateLocalKeypair(): {
  publicKey: string;
  secretKey: string;
} {
  const keypair = StellarSdk.Keypair.random();
  return {
    publicKey: keypair.publicKey(),
    secretKey: keypair.secret(),
  };
}

export async function saveLocalKeypair(secretKey: string): Promise<void> {
  await AsyncStorage.setItem(KEY_STORAGE_KEY, secretKey);
}

export async function getLocalKeypair(): Promise<StellarSdk.Keypair | null> {
  try {
    const secret = await AsyncStorage.getItem(KEY_STORAGE_KEY);
    if (!secret) return null;
    return StellarSdk.Keypair.fromSecret(secret);
  } catch {
    return null;
  }
}

export async function connectLocalWallet(): Promise<StellarWalletState> {
  const kp = await getLocalKeypair();
  if (!kp) {
    throw new Error("No local keypair found. Generate one first.");
  }
  return { publicKey: kp.publicKey(), isConnected: true, source: "local" };
}

export interface TransactionFeeEstimate {
  baseFee: string;
  minResourceFee: string;
  totalFee: string;
  cpuInsns?: string;
  memBytes?: string;
}

export async function getAccountBalance(
  publicKey: string
): Promise<{ balance: string; asset: string }[]> {
  try {
    const account = await server.loadAccount(publicKey);
    return account.balances.map((b: any) => ({
      balance: b.balance,
      asset: b.asset_type === "native" ? "XLM" : b.asset_code || "XLM",
    }));
  } catch {
    return [];
  }
}

function parseNumericValue(value: unknown): string | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value.toString();
  }

  if (typeof value === "string") {
    const trimmed = value.trim();
    if (!trimmed) return null;
    const numeric = Number(trimmed);
    return Number.isFinite(numeric) ? numeric.toString() : null;
  }

  if (Array.isArray(value) && value.length > 0) {
    return parseNumericValue(value[0]);
  }

  if (typeof value === "object" && value !== null) {
    const maybe = value as Record<string, unknown>;
    for (const key of [
      "value",
      "amount",
      "fee",
      "feeCharged",
      "fee_charged",
      "minResourceFee",
      "min_resource_fee",
      "cpuInsns",
      "cpu_insns",
      "memBytes",
      "mem_bytes",
    ]) {
      if (Object.prototype.hasOwnProperty.call(maybe, key)) {
        const parsed = parseNumericValue(maybe[key]);
        if (parsed) return parsed;
      }
    }
  }

  return null;
}

export async function estimateTransactionFee(
  txXdr: string
): Promise<TransactionFeeEstimate> {
  const baseFee = await server.fetchBaseFee();
  const sorobanRpcUrl =
    (process.env.EXPO_PUBLIC_SOROBAN_RPC_URL ||
      (STELLAR_NETWORK === "PUBLIC"
        ? "https://soroban.stellar.org"
        : "https://soroban-testnet.stellar.org")) ||
    (STELLAR_NETWORK === "PUBLIC"
      ? "https://soroban.stellar.org"
      : "https://soroban-testnet.stellar.org");

  const response = await fetch(sorobanRpcUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "simulateTransaction",
      params: { transaction: txXdr },
    }),
  });

  if (!response.ok) {
    throw new Error(`Soroban simulation failed with HTTP ${response.status}`);
  }

  const body = await response.json();
  const result = body?.result ?? {};
  const minResourceFee =
    parseNumericValue(result.minResourceFee) ??
    parseNumericValue(result.min_resource_fee) ??
    parseNumericValue(result.cost?.feeCharged) ??
    parseNumericValue(result.cost?.fee_charged) ??
    parseNumericValue(result.feeCharged) ??
    parseNumericValue(result.fee_charged) ??
    parseNumericValue(result.cost?.feeChanges) ??
    parseNumericValue(result.cost?.fee_changes) ??
    "0";
  const cpuInsns =
    parseNumericValue(result.cost?.cpuInsns) ??
    parseNumericValue(result.cost?.cpu_insns) ??
    undefined;
  const memBytes =
    parseNumericValue(result.cost?.memBytes) ??
    parseNumericValue(result.cost?.mem_bytes) ??
    undefined;

  const baseFeeValue = Number(baseFee) || 0;
  const minResourceFeeValue = Number(minResourceFee) || 0;

  return {
    baseFee: baseFeeValue.toString(),
    minResourceFee: minResourceFeeValue.toString(),
    totalFee: (baseFeeValue + minResourceFeeValue).toString(),
    cpuInsns,
    memBytes,
  };
}

export async function buildPaymentEnvelope(
  sourcePublicKey: string,
  destination: string,
  amount: string,
  assetCode: string = "XLM",
  assetIssuer?: string,
  memo?: string
): Promise<string> {
  const sourceAccount = await server.loadAccount(sourcePublicKey);

  let asset: StellarSdk.Asset;
  if (assetCode === "XLM") {
    asset = StellarSdk.Asset.native();
  } else if (assetIssuer) {
    asset = new StellarSdk.Asset(assetCode, assetIssuer);
  } else {
    asset = StellarSdk.Asset.native();
  }

  const fee = await server.fetchBaseFee();

  const builder = new StellarSdk.TransactionBuilder(sourceAccount, {
    fee: fee.toString(),
    networkPassphrase,
  });

  const paymentOp = StellarSdk.Operation.payment({
    destination,
    asset,
    amount,
  });

  builder.addOperation(paymentOp);

  if (memo) {
    builder.addMemo(StellarSdk.Memo.text(memo));
  }

  const transaction = builder.setTimeout(300).build();

  return transaction.toXDR();
}

/**
 * How many times over the current network base fee we are willing to pay when
 * speeding up a stuck transaction. A higher fee increases the chance the
 * transaction is included in the next ledger.
 */
export const SPEED_UP_FEE_MULTIPLIER = 2;

/**
 * fee-bump helper — wraps an existing (stuck) transaction envelope in a
 * `FeeBumpTransaction` that pays a higher max fee so the network prioritises
 * resubmitting it.
 *
 * @param innerTxXdr base64 XDR of the stuck transaction to bump (the inner tx)
 * @param feeSource  account id (G.../M...) that pays for the fee bump
 * @param baseFee    max fee willing to pay per operation, in stroops
 */
export function buildFeeBumpEnvelope(
  innerTxXdr: string,
  feeSource: string,
  baseFee: string
): StellarSdk.FeeBumpTransaction {
  const innerTx = new StellarSdk.Transaction(innerTxXdr, networkPassphrase);

  return StellarSdk.TransactionBuilder.buildFeeBumpTransaction(
    feeSource,
    baseFee,
    innerTx,
    networkPassphrase
  );
}

export async function signAndSubmitTransaction(
  txXdr: string,
  source: "freighter" | "albedo" | "local"
): Promise<string> {
  let signedXdr: string;

  if (source === "freighter") {
    signedXdr = await signWithFreighter(txXdr);
  } else if (source === "albedo") {
    signedXdr = await signWithAlbedo(txXdr);
  } else {
    const kp = await getLocalKeypair();
    if (!kp) throw new Error("No local keypair available");
    const transaction = new StellarSdk.Transaction(txXdr, networkPassphrase);
    transaction.sign(kp);
    signedXdr = transaction.toXDR();
  }

  const envelope = StellarSdk.xdr.TransactionEnvelope.fromXDR(
    signedXdr,
    StellarSdk.xdr.TransactionEnvelopeType.envelopeTypeTx
  );
  const transaction = new StellarSdk.Transaction(envelope, networkPassphrase);
  const result = await server.submitTransaction(transaction);

  return result.hash;
}

/**
 * Speed up a stuck transaction by fee-bumping it with a higher max fee and
 * resubmitting it to the Stellar network.
 *
 * @param txXdr    base64 XDR of the stuck transaction to bump
 * @param source   the wallet backend used to sign the enclosing fee bump
 * @param feeSource the account that pays for the fee bump (defaults to the
 *                 signing wallet's own account)
 */
export async function speedUpTransaction(
  txXdr: string,
  source: "freighter" | "albedo" | "local",
  feeSource?: string
): Promise<string> {
  // Resolve the fee-paying account from the active wallet if not provided.
  let feePayer = feeSource;
  if (!feePayer) {
    if (source === "freighter") {
      feePayer = (await connectFreighter()).publicKey;
    } else if (source === "albedo") {
      feePayer = (await connectAlbedo()).publicKey;
    } else {
      const kp = await getLocalKeypair();
      if (!kp) throw new Error("No local keypair available");
      feePayer = kp.publicKey();
    }
  }

  // Pay a higher max fee to prioritise inclusion.
  const baseFee = await server.fetchBaseFee();
  const minimumFee = Number(StellarSdk.BASE_FEE) || 100;
  const bumpedFee = Math.max(
    baseFee * SPEED_UP_FEE_MULTIPLIER,
    minimumFee * SPEED_UP_FEE_MULTIPLIER
  );

  const feeBumpTransaction = buildFeeBumpEnvelope(
    txXdr,
    feePayer,
    bumpedFee.toString()
  );

  let signedXdr: string;
  if (source === "freighter") {
    signedXdr = await signWithFreighter(feeBumpTransaction.toXDR());
  } else if (source === "albedo") {
    signedXdr = await signWithAlbedo(feeBumpTransaction.toXDR());
  } else {
    const kp = await getLocalKeypair();
    if (!kp) throw new Error("No local keypair available");
    feeBumpTransaction.sign(kp);
    signedXdr = feeBumpTransaction.toXDR();
  }

  // Rebuild from the signed envelope (the SDK auto-detects fee bumps) and
  // resubmit the fee-bumped transaction to the Stellar network.
  const submitted = StellarSdk.TransactionBuilder.fromXDR(
    signedXdr,
    networkPassphrase
  );
  const result = await server.submitTransaction(submitted);

  return result.hash;
}

export async function submitPayment(
  destination: string,
  amount: string,
  source: "freighter" | "albedo" | "local",
  sourcePublicKey: string,
  assetCode?: string,
  assetIssuer?: string,
  memo?: string
): Promise<{ hash: string; destination: string; amount: string }> {
  const txXdr = await buildPaymentEnvelope(
    sourcePublicKey,
    destination,
    amount,
    assetCode,
    assetIssuer,
    memo
  );

  const hash = await signAndSubmitTransaction(txXdr, source);

  await fetchWithRetry(`${API_BASE}/api/transactions`, {
    method: "POST",
    body: JSON.stringify({
      hash,
      destination,
      amount,
      asset: assetCode || "XLM",
      memo,
    }),
  }).catch(() => {});

  return { hash, destination, amount };
}
