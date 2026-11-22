/**
 * SDP Deep Link Parameter Extractor
 *
 * Parses Zaps SDP deep link URLs of the form:
 *   zaps://sdp/pay?token=<token>&amount=<amount>
 *
 * Returns the extracted token and amount, or null for malformed URLs.
 */

export interface SdpDeepLinkParams {
  token: string;
  amount: number;
}

/**
 * Parse an SDP deep link URL string and extract the `token` and `amount` query
 * parameters.  Returns `null` when the URL is malformed or any required
 * parameter is missing / invalid.
 */
export function parseSdpDeepLink(url: string): SdpDeepLinkParams | null {
  if (!url || typeof url !== 'string') {
    return null;
  }

  try {
    // Accept both custom scheme (zaps://) and https:// fallbacks
    let urlString = url.trim();

    // Basic sanity check – must look like a URL with a query string
    if (!urlString.includes('?')) {
      return null;
    }

    const parsed = new URL(urlString);
    const token = parsed.searchParams.get('token');
    const amountRaw = parsed.searchParams.get('amount');

    if (!token) {
      return null;
    }

    if (amountRaw === null || amountRaw === undefined || amountRaw === '') {
      return null;
    }

    const amount = Number(amountRaw);

    if (Number.isNaN(amount) || amount <= 0) {
      return null;
    }

    return { token, amount };
  } catch {
    return null;
  }
}
