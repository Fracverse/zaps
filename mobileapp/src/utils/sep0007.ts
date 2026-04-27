/**
 * SEP-0007 URI parser and validator
 * https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0007.md
 */

export interface Sep0007PaymentParams {
  destination: string;
  amount?: string;
  asset_code?: string;
  asset_issuer?: string;
  memo?: string;
  memo_type?: "text" | "id" | "hash" | "return";
  callback?: string;
  msg?: string;
  network_passphrase?: string;
  origin_domain?: string;
  signature?: string;
}

export interface Sep0007TxParams {
  xdr: string;
  callback?: string;
  pubkey?: string;
  msg?: string;
  network_passphrase?: string;
  origin_domain?: string;
  signature?: string;
}

export type Sep0007Operation = "pay" | "tx";

export interface Sep0007ParseResult {
  operation: Sep0007Operation;
  params: Sep0007PaymentParams | Sep0007TxParams;
}

export interface ParseError {
  valid: false;
  error: string;
}

export type ParseResult = ({ valid: true } & Sep0007ParseResult) | ParseError;

// Stellar G-address: starts with G, 56 chars, base32
const STELLAR_ADDRESS_REGEX = /^G[A-Z2-7]{55}$/;
// Federated address: user*domain.tld
const FEDERATED_ADDRESS_REGEX = /^[^*]+\*[^*]+\.[^*]+$/;

export function isStellarAddress(address: string): boolean {
  return STELLAR_ADDRESS_REGEX.test(address);
}

export function isFederatedAddress(address: string): boolean {
  return FEDERATED_ADDRESS_REGEX.test(address);
}

export function isValidStellarDestination(destination: string): boolean {
  return isStellarAddress(destination) || isFederatedAddress(destination);
}

/**
 * Parse a SEP-0007 URI string.
 * Returns a typed result with valid flag.
 */
export function parseSep0007Uri(uri: string): ParseResult {
  if (!uri || typeof uri !== "string") {
    return { valid: false, error: "Empty or invalid URI" };
  }

  const trimmed = uri.trim();

  if (!trimmed.startsWith("web+stellar:")) {
    return {
      valid: false,
      error: "Not a SEP-0007 URI (missing web+stellar: scheme)",
    };
  }

  const withoutScheme = trimmed.slice("web+stellar:".length);
  const slashIndex = withoutScheme.indexOf("?");
  const operationPart =
    slashIndex === -1 ? withoutScheme : withoutScheme.slice(0, slashIndex);
  const queryPart =
    slashIndex === -1 ? "" : withoutScheme.slice(slashIndex + 1);

  const operation = operationPart as Sep0007Operation;

  if (operation !== "pay" && operation !== "tx") {
    return {
      valid: false,
      error: `Unsupported SEP-0007 operation: "${operation}". Expected "pay" or "tx".`,
    };
  }

  const rawParams: Record<string, string> = {};
  if (queryPart) {
    for (const pair of queryPart.split("&")) {
      const eqIdx = pair.indexOf("=");
      if (eqIdx === -1) continue;
      const key = decodeURIComponent(pair.slice(0, eqIdx));
      const value = decodeURIComponent(pair.slice(eqIdx + 1));
      rawParams[key] = value;
    }
  }

  if (operation === "pay") {
    const destination = rawParams["destination"];
    if (!destination) {
      return { valid: false, error: "Missing required field: destination" };
    }
    if (!isValidStellarDestination(destination)) {
      return {
        valid: false,
        error: `Invalid Stellar destination address: "${destination}"`,
      };
    }

    const amount = rawParams["amount"];
    if (amount !== undefined) {
      const parsed = parseFloat(amount);
      if (isNaN(parsed) || parsed < 0) {
        return { valid: false, error: `Invalid amount: "${amount}"` };
      }
    }

    const memo_type = rawParams[
      "memo_type"
    ] as Sep0007PaymentParams["memo_type"];
    if (
      memo_type !== undefined &&
      !["text", "id", "hash", "return"].includes(memo_type)
    ) {
      return { valid: false, error: `Invalid memo_type: "${memo_type}"` };
    }

    const params: Sep0007PaymentParams = {
      destination,
      ...(amount !== undefined && { amount }),
      ...(rawParams["asset_code"] && { asset_code: rawParams["asset_code"] }),
      ...(rawParams["asset_issuer"] && {
        asset_issuer: rawParams["asset_issuer"],
      }),
      ...(rawParams["memo"] && { memo: rawParams["memo"] }),
      ...(memo_type && { memo_type }),
      ...(rawParams["callback"] && { callback: rawParams["callback"] }),
      ...(rawParams["msg"] && { msg: rawParams["msg"] }),
      ...(rawParams["network_passphrase"] && {
        network_passphrase: rawParams["network_passphrase"],
      }),
      ...(rawParams["origin_domain"] && {
        origin_domain: rawParams["origin_domain"],
      }),
      ...(rawParams["signature"] && { signature: rawParams["signature"] }),
    };

    return { valid: true, operation: "pay", params };
  }

  // operation === "tx"
  const xdr = rawParams["xdr"];
  if (!xdr) {
    return { valid: false, error: "Missing required field: xdr" };
  }

  const params: Sep0007TxParams = {
    xdr,
    ...(rawParams["callback"] && { callback: rawParams["callback"] }),
    ...(rawParams["pubkey"] && { pubkey: rawParams["pubkey"] }),
    ...(rawParams["msg"] && { msg: rawParams["msg"] }),
    ...(rawParams["network_passphrase"] && {
      network_passphrase: rawParams["network_passphrase"],
    }),
    ...(rawParams["origin_domain"] && {
      origin_domain: rawParams["origin_domain"],
    }),
    ...(rawParams["signature"] && { signature: rawParams["signature"] }),
  };

  return { valid: true, operation: "tx", params };
}

/**
 * Build a SEP-0007 pay URI from params.
 */
export function buildSep0007PayUri(params: Sep0007PaymentParams): string {
  const parts: string[] = [];

  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== null && value !== "") {
      parts.push(
        `${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`
      );
    }
  }

  return `web+stellar:pay?${parts.join("&")}`;
}

/**
 * Detect if a scanned string is a SEP-0007 URI, a plain Stellar address,
 * or something else entirely.
 */
export type QrContentType = "sep0007" | "stellar_address" | "unknown";

export function detectQrContentType(content: string): QrContentType {
  if (!content) return "unknown";
  if (content.startsWith("web+stellar:")) return "sep0007";
  if (isValidStellarDestination(content.trim())) return "stellar_address";
  return "unknown";
}
