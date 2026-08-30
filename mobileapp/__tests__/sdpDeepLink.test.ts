import { parseSdpDeepLink } from '../utils/sdpDeepLink';

describe('parseSdpDeepLink', () => {
  // ── Valid URLs ──────────────────────────────────────────────────────────

  it('parses a standard SDP deep link with token and amount', () => {
    const result = parseSdpDeepLink(
      'zaps://sdp/pay?token=abc123&amount=100',
    );
    expect(result).toEqual({ token: 'abc123', amount: 100 });
  });

  it('parses a deep link with a decimal amount', () => {
    const result = parseSdpDeepLink(
      'zaps://sdp/pay?token=tok_99&amount=49.99',
    );
    expect(result).toEqual({ token: 'tok_99', amount: 49.99 });
  });

  it('parses a deep link with extra query parameters (ignores them)', () => {
    const result = parseSdpDeepLink(
      'zaps://sdp/pay?token=xyz&amount=250&ref=123&source=qr',
    );
    expect(result).toEqual({ token: 'xyz', amount: 250 });
  });

  it('parses an HTTPS URL variant', () => {
    const result = parseSdpDeepLink(
      'https://zaps.app/sdp/pay?token=secure_token_1&amount=1000',
    );
    expect(result).toEqual({ token: 'secure_token_1', amount: 1000 });
  });

  it('handles URL-encoded token values', () => {
    const result = parseSdpDeepLink(
      'zaps://sdp/pay?token=hello%20world&amount=55',
    );
    expect(result).toEqual({ token: 'hello world', amount: 55 });
  });

  it('handles leading/trailing whitespace in the URL', () => {
    const result = parseSdpDeepLink(
      '  zaps://sdp/pay?token=trim_me&amount=10  ',
    );
    expect(result).toEqual({ token: 'trim_me', amount: 10 });
  });

  it('returns amount as number, not string', () => {
    const result = parseSdpDeepLink(
      'zaps://sdp/pay?token=num&amount=777',
    );
    expect(typeof result!.amount).toBe('number');
  });

  // ── Malformed / missing-parameter URLs ──────────────────────────────────

  it('returns null for an empty string', () => {
    expect(parseSdpDeepLink('')).toBeNull();
  });

  it('returns null for a non-string input', () => {
    expect(parseSdpDeepLink(undefined as unknown as string)).toBeNull();
    expect(parseSdpDeepLink(null as unknown as string)).toBeNull();
  });

  it('returns null for a URL without query parameters', () => {
    expect(parseSdpDeepLink('zaps://sdp/pay')).toBeNull();
  });

  it('returns null when the token parameter is missing', () => {
    expect(parseSdpDeepLink('zaps://sdp/pay?amount=100')).toBeNull();
  });

  it('returns null when the amount parameter is missing', () => {
    expect(parseSdpDeepLink('zaps://sdp/pay?token=abc')).toBeNull();
  });

  it('returns null when token is present but empty', () => {
    expect(
      parseSdpDeepLink('zaps://sdp/pay?token=&amount=100'),
    ).toBeNull();
  });

  it('returns null when amount is present but empty', () => {
    expect(
      parseSdpDeepLink('zaps://sdp/pay?token=abc&amount='),
    ).toBeNull();
  });

  it('returns null when amount is not a number (NaN)', () => {
    expect(
      parseSdpDeepLink('zaps://sdp/pay?token=abc&amount=notanumber'),
    ).toBeNull();
  });

  it('returns null when amount is zero', () => {
    expect(
      parseSdpDeepLink('zaps://sdp/pay?token=abc&amount=0'),
    ).toBeNull();
  });

  it('returns null when amount is negative', () => {
    expect(
      parseSdpDeepLink('zaps://sdp/pay?token=abc&amount=-50'),
    ).toBeNull();
  });

  it('returns null for a completely malformed URL string', () => {
    expect(parseSdpDeepLink('not-a-url-at-all')).toBeNull();
  });

  it('returns null for random gibberish', () => {
    expect(parseSdpDeepLink('::::')).toBeNull();
  });
});
