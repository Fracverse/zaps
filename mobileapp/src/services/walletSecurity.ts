/**
 * walletSecurity.ts
 *
 * Secure key management service for the Zaps wallet.
 *
 * Security guarantees:
 *  - Private keys and mnemonics are stored ONLY in expo-secure-store (device keychain / Keystore).
 *  - Keys are NEVER sent to the backend.
 *  - Clipboard is cleared automatically after 30 seconds.
 *  - Multi-account support via account index.
 *  - Key rotation replaces the stored key for a given account index.
 */

import { createHmac } from "crypto";
import * as SecureStore from "expo-secure-store";
import * as Clipboard from "expo-clipboard";
import * as bip39 from "bip39";
import { Keypair } from "@stellar/stellar-sdk";
import { Buffer } from "buffer";

// ── Storage key helpers ───────────────────────────────────────────────────────

const MNEMONIC_KEY = "wallet_mnemonic_v1";

const secretKeyStorageKey = (accountIndex: number): string =>
  `wallet_secret_key_v1_${accountIndex}`;

const publicKeyStorageKey = (accountIndex: number): string =>
  `wallet_public_key_v1_${accountIndex}`;

const ACCOUNT_COUNT_KEY = "wallet_account_count_v1";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface WalletAccount {
  accountIndex: number;
  publicKey: string;
}

export interface GeneratedWallet {
  mnemonic: string;
  accounts: WalletAccount[];
}

// ── Mnemonic generation ───────────────────────────────────────────────────────

/**
 * Generate a cryptographically secure BIP39 mnemonic phrase.
 * @param wordCount 12 or 24 words (128-bit or 256-bit entropy).
 */
export function generateMnemonic(wordCount: 12 | 24 = 24): string {
  const strength = wordCount === 12 ? 128 : 256;
  return bip39.generateMnemonic(strength);
}

/** Validate a BIP39 mnemonic phrase. */
export function validateMnemonic(mnemonic: string): boolean {
  const normalized = mnemonic.trim().toLowerCase().replace(/\s+/g, " ");
  return bip39.validateMnemonic(normalized);
}

// ── Key derivation ────────────────────────────────────────────────────────────

/**
 * Derive a Stellar Keypair from a BIP39 mnemonic at the given account index.
 * Derivation path: m/44'/148'/{accountIndex}'  (SEP-0005 standard)
 */
export async function deriveKeypairFromMnemonic(
  mnemonic: string,
  accountIndex: number = 0
): Promise<Keypair> {
  const normalized = mnemonic.trim().toLowerCase().replace(/\s+/g, " ");
  if (!bip39.validateMnemonic(normalized)) {
    throw new Error("Invalid mnemonic phrase");
  }
  const seed = await bip39.mnemonicToSeed(normalized);
  const derivedKey = deriveStellarKey(seed, accountIndex);
  return Keypair.fromRawEd25519Seed(derivedKey);
}

/**
 * Derive a Stellar key from a BIP39 seed using SEP-0005 path m/44'/148'/{index}'.
 * Uses HMAC-SHA512 based key derivation (Ed25519 HD wallet standard).
 */
function deriveStellarKey(seed: Buffer, accountIndex: number): Buffer {
  const masterHmac = createHmac("sha512", "ed25519 seed");
  masterHmac.update(seed);
  const masterResult = masterHmac.digest();
  let key: Buffer = Buffer.from(masterResult.subarray(0, 32));
  let chainCode: Buffer = Buffer.from(masterResult.subarray(32));

  // Path: 44' -> 148' -> accountIndex' (all hardened)
  const pathComponents = [
    0x8000002c,
    0x80000094,
    0x80000000 + accountIndex,
  ];

  for (const index of pathComponents) {
    const data = Buffer.alloc(37);
    data[0] = 0x00;
    key.copy(data, 1);
    data.writeUInt32BE(index, 33);

    const hmac = createHmac("sha512", chainCode);
    hmac.update(data);
    const result = hmac.digest();
    key = Buffer.from(result.subarray(0, 32));
    chainCode = Buffer.from(result.subarray(32));
  }

  return key;
}

// ── Secure storage ────────────────────────────────────────────────────────────

/** Store a mnemonic phrase securely in the device keychain. */
export async function storeMnemonic(mnemonic: string): Promise<void> {
  await SecureStore.setItemAsync(MNEMONIC_KEY, mnemonic, {
    keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
  });
}

/** Retrieve the stored mnemonic phrase. Returns null if none stored. */
export async function retrieveMnemonic(): Promise<string | null> {
  return SecureStore.getItemAsync(MNEMONIC_KEY, {
    keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
  });
}

/** Store a Stellar keypair for a given account index. */
export async function storeKeypair(
  keypair: Keypair,
  accountIndex: number
): Promise<void> {
  await SecureStore.setItemAsync(
    secretKeyStorageKey(accountIndex),
    keypair.secret(),
    { keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY }
  );
  await SecureStore.setItemAsync(
    publicKeyStorageKey(accountIndex),
    keypair.publicKey(),
    { keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY }
  );
}

/** Retrieve the Stellar Keypair for a given account index. Returns null if not stored. */
export async function retrieveKeypair(
  accountIndex: number
): Promise<Keypair | null> {
  const secret = await SecureStore.getItemAsync(
    secretKeyStorageKey(accountIndex),
    { keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY }
  );
  if (!secret) return null;
  return Keypair.fromSecret(secret);
}

/** Retrieve only the public key for a given account index. */
export async function retrievePublicKey(
  accountIndex: number
): Promise<string | null> {
  return SecureStore.getItemAsync(publicKeyStorageKey(accountIndex), {
    keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
  });
}

// ── Account management ────────────────────────────────────────────────────────

/** Get the number of accounts created on this device. */
export async function getAccountCount(): Promise<number> {
  const raw = await SecureStore.getItemAsync(ACCOUNT_COUNT_KEY);
  return raw ? parseInt(raw, 10) : 0;
}

async function incrementAccountCount(): Promise<number> {
  const current = await getAccountCount();
  const next = current + 1;
  await SecureStore.setItemAsync(ACCOUNT_COUNT_KEY, String(next));
  return next;
}

/** List all stored wallet accounts (public keys only — no secrets exposed). */
export async function listAccounts(): Promise<WalletAccount[]> {
  const count = await getAccountCount();
  const accounts: WalletAccount[] = [];
  for (let i = 0; i < count; i++) {
    const publicKey = await retrievePublicKey(i);
    if (publicKey) {
      accounts.push({ accountIndex: i, publicKey });
    }
  }
  return accounts;
}

// ── Full wallet setup ─────────────────────────────────────────────────────────

/**
 * Create a brand-new wallet, derive account 0, and store everything securely.
 * Returns the mnemonic (shown ONCE to the user) and the first account.
 */
export async function createNewWallet(
  wordCount: 12 | 24 = 24
): Promise<GeneratedWallet> {
  const mnemonic = generateMnemonic(wordCount);
  const keypair = await deriveKeypairFromMnemonic(mnemonic, 0);

  await storeMnemonic(mnemonic);
  await storeKeypair(keypair, 0);
  await SecureStore.setItemAsync(ACCOUNT_COUNT_KEY, "1");

  return {
    mnemonic,
    accounts: [{ accountIndex: 0, publicKey: keypair.publicKey() }],
  };
}

/** Add a new account derived from the existing stored mnemonic. */
export async function addAccount(): Promise<WalletAccount> {
  const mnemonic = await retrieveMnemonic();
  if (!mnemonic) {
    throw new Error("No wallet found. Please create or restore a wallet first.");
  }
  const currentCount = await getAccountCount();
  const newIndex = currentCount;
  const keypair = await deriveKeypairFromMnemonic(mnemonic, newIndex);
  await storeKeypair(keypair, newIndex);
  await incrementAccountCount();
  return { accountIndex: newIndex, publicKey: keypair.publicKey() };
}

// ── Wallet recovery ───────────────────────────────────────────────────────────

/**
 * Restore a wallet from a mnemonic phrase.
 * Validates the phrase, derives account 0, and stores everything securely.
 */
export async function restoreWalletFromMnemonic(
  mnemonic: string
): Promise<WalletAccount> {
  const normalized = mnemonic.trim().toLowerCase().replace(/\s+/g, " ");
  if (!bip39.validateMnemonic(normalized)) {
    throw new Error(
      "Invalid mnemonic phrase. Please check your words and try again."
    );
  }
  const keypair = await deriveKeypairFromMnemonic(normalized, 0);
  await storeMnemonic(normalized);
  await storeKeypair(keypair, 0);
  await SecureStore.setItemAsync(ACCOUNT_COUNT_KEY, "1");
  return { accountIndex: 0, publicKey: keypair.publicKey() };
}

// ── Key rotation ──────────────────────────────────────────────────────────────

/**
 * Rotate the key for a given account index.
 * Re-derives from the stored mnemonic and overwrites the stored keypair.
 */
export async function rotateKey(accountIndex: number): Promise<WalletAccount> {
  const mnemonic = await retrieveMnemonic();
  if (!mnemonic) throw new Error("No wallet mnemonic found.");
  const keypair = await deriveKeypairFromMnemonic(mnemonic, accountIndex);
  await storeKeypair(keypair, accountIndex);
  return { accountIndex, publicKey: keypair.publicKey() };
}

// ── Wallet existence check ────────────────────────────────────────────────────

/** Returns true if a wallet has already been set up on this device. */
export async function hasWallet(): Promise<boolean> {
  const count = await getAccountCount();
  return count > 0;
}

// ── Clipboard utilities ───────────────────────────────────────────────────────

const CLIPBOARD_CLEAR_DELAY_MS = 30_000; // 30 seconds

let clipboardClearTimer: ReturnType<typeof setTimeout> | null = null;

/**
 * Copy a sensitive value to the clipboard and auto-clear after 30 seconds.
 * Returns a cancel function that clears immediately.
 */
export function copyWithAutoClear(value: string): () => void {
  if (clipboardClearTimer !== null) {
    clearTimeout(clipboardClearTimer);
    clipboardClearTimer = null;
  }

  Clipboard.setStringAsync(value);

  clipboardClearTimer = setTimeout(() => {
    Clipboard.setStringAsync("");
    clipboardClearTimer = null;
  }, CLIPBOARD_CLEAR_DELAY_MS);

  return () => {
    if (clipboardClearTimer !== null) {
      clearTimeout(clipboardClearTimer);
      clipboardClearTimer = null;
    }
    Clipboard.setStringAsync("");
  };
}

/** Immediately clear the clipboard and cancel any pending auto-clear timer. */
export async function clearClipboard(): Promise<void> {
  if (clipboardClearTimer !== null) {
    clearTimeout(clipboardClearTimer);
    clipboardClearTimer = null;
  }
  await Clipboard.setStringAsync("");
}

// ── Secure deletion ───────────────────────────────────────────────────────────

/**
 * Delete ALL wallet data from secure storage.
 * WARNING: irreversible — user must have backed up their mnemonic first.
 */
export async function deleteWallet(): Promise<void> {
  const count = await getAccountCount();
  for (let i = 0; i < count; i++) {
    await SecureStore.deleteItemAsync(secretKeyStorageKey(i));
    await SecureStore.deleteItemAsync(publicKeyStorageKey(i));
  }
  await SecureStore.deleteItemAsync(MNEMONIC_KEY);
  await SecureStore.deleteItemAsync(ACCOUNT_COUNT_KEY);
}
