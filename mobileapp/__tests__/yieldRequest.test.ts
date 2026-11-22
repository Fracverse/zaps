import {
  RequestTimeoutError,
  describeYieldError,
  withTimeout,
} from "../src/utils/yieldRequest";

describe("yield request helpers (#906)", () => {
  afterEach(() => {
    jest.useRealTimers();
  });

  it("resolves with the value when the request finishes in time", async () => {
    await expect(withTimeout(Promise.resolve("ok"), 1000)).resolves.toBe("ok");
  });

  it("rejects with a RequestTimeoutError when the request is too slow", async () => {
    jest.useFakeTimers();
    const never = new Promise<string>(() => undefined);
    const result = withTimeout(never, 4500);
    const assertion =
      expect(result).rejects.toBeInstanceOf(RequestTimeoutError);
    jest.advanceTimersByTime(4500);
    await assertion;
  });

  it("passes through the original rejection and clears the timer", async () => {
    jest.useFakeTimers();
    await expect(
      withTimeout(Promise.reject(new Error("HTTP 503")), 4500)
    ).rejects.toThrow("HTTP 503");
    expect(jest.getTimerCount()).toBe(0);
  });

  it("describes timeouts, server errors and unknown failures", () => {
    expect(describeYieldError(new RequestTimeoutError())).toBe(
      "The request timed out."
    );
    expect(describeYieldError(new Error("HTTP 503"))).toBe(
      "The server could not complete the request."
    );
    expect(describeYieldError(new Error("Network request failed"))).toBe(
      "Unable to load yield details right now."
    );
    expect(describeYieldError("boom")).toBe(
      "Unable to load yield details right now."
    );
  });
});
