import { parseSdpClaimUrl, isValidSdpToken } from "../src/utils/sdpDeepLink";

describe("parseSdpClaimUrl", () => {
  it("parses a custom-scheme claim link with a query-param token", () => {
    const result = parseSdpClaimUrl("zaps://claim?token=abc123-def456");
    expect(result).toEqual({ valid: true, token: "abc123-def456" });
  });

  it("parses a custom-scheme claim link with a path-based token", () => {
    const result = parseSdpClaimUrl("zaps://claim/abc123-def456");
    expect(result).toEqual({ valid: true, token: "abc123-def456" });
  });

  it("parses an https invite link with a query-param token", () => {
    const result = parseSdpClaimUrl("https://zaps.app/claim?token=abc123-def456");
    expect(result).toEqual({ valid: true, token: "abc123-def456" });
  });

  it("accepts alternate accepted token param names", () => {
    const result = parseSdpClaimUrl("zaps://claim?sdp_token=xyz789token");
    expect(result).toEqual({ valid: true, token: "xyz789token" });
  });

  it("ignores links that are not SDP claim links", () => {
    const result = parseSdpClaimUrl("zaps://home");
    expect(result.valid).toBe(false);
  });

  it("rejects a claim link missing a token", () => {
    const result = parseSdpClaimUrl("zaps://claim");
    expect(result).toEqual({ valid: false, error: "Missing claim token in URL" });
  });

  it("rejects a claim link with a malformed token", () => {
    const result = parseSdpClaimUrl("zaps://claim?token=<script>bad</script>");
    expect(result.valid).toBe(false);
  });

  it("rejects empty or null input", () => {
    expect(parseSdpClaimUrl("").valid).toBe(false);
    expect(parseSdpClaimUrl(null).valid).toBe(false);
    expect(parseSdpClaimUrl(undefined).valid).toBe(false);
  });
});

describe("isValidSdpToken", () => {
  it("accepts well-formed tokens", () => {
    expect(isValidSdpToken("abc123-DEF.456_ghi")).toBe(true);
  });

  it("rejects tokens that are too short", () => {
    expect(isValidSdpToken("ab1")).toBe(false);
  });

  it("rejects tokens with disallowed characters", () => {
    expect(isValidSdpToken("abc/../etc")).toBe(false);
    expect(isValidSdpToken("<script>")).toBe(false);
  });
});
