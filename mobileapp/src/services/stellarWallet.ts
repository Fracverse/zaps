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
