export const YIELD_REQUEST_TIMEOUT_MS = 4500;

export class RequestTimeoutError extends Error {
  constructor(message = "Request timed out.") {
    super(message);
    this.name = "RequestTimeoutError";
  }
}

/** Reject with a RequestTimeoutError if `promise` takes longer than `timeoutMs`. */
export async function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new RequestTimeoutError()), timeoutMs);
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    clearTimeout(timer);
  }
}

/** User-facing copy for a failed yield request; always ends with a period. */
export function describeYieldError(error: unknown): string {
  if (error instanceof RequestTimeoutError) {
    return "The request timed out.";
  }
  const message = error instanceof Error ? error.message : "";
  if (/^HTTP \d+/.test(message)) {
    return "The server could not complete the request.";
  }
  return "Unable to load yield details right now.";
}
