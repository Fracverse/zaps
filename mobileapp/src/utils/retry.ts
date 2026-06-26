// Retry utility with exponential backoff
export interface RetryOptions {
  maxRetries?: number;
  baseDelay?: number;
  maxDelay?: number;
  backoffFactor?: number;
  onRetry?: (attempt: number, error: any) => void;
}

export const retryWithBackoff = async <T>(
  fn: () => Promise<T>,
  options: RetryOptions = {}
): Promise<T> => {
  const {
    maxRetries = 3,
    baseDelay = 1000,
    maxDelay = 10000,
    backoffFactor = 2,
    onRetry,
  } = options;

  let lastError: any;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error;

      if (attempt === maxRetries) {
        throw lastError;
      }

      // Calculate delay with exponential backoff
      const delay = Math.min(
        baseDelay * Math.pow(backoffFactor, attempt),
        maxDelay
      );

      // Notify about retry
      onRetry?.(attempt + 1, error);

      // Wait before retry
      await new Promise((resolve) => setTimeout(resolve, delay));
    }
  }

  throw lastError;
};

// Network request wrapper with retry logic
export const fetchWithRetry = async (
  url: string,
  options: RequestInit = {},
  retryOptions: RetryOptions = {}
): Promise<Response> => {
  return retryWithBackoff(
    async () => {
      const response = await fetch(url, {
        ...options,
        headers: {
          "Content-Type": "application/json",
          ...options.headers,
        },
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      return response;
    },
    {
      maxRetries: 3,
      baseDelay: 1000,
      maxDelay: 5000,
      onRetry: (attempt, error) => {
        console.warn(`Retry attempt ${attempt} for ${url}:`, error);
        if (typeof global !== "undefined" && global.toast) {
          global.toast.warning(`Network error. Retrying... (${attempt}/3)`, {
            label: "Cancel",
            onPress: () => {
              // This would need to be implemented to cancel the retry
              console.log("Retry cancelled by user");
            },
          });
        }
      },
      ...retryOptions,
    }
  );
};

// Safe async function wrapper that shows toast notifications
export const safeAsyncCall = async <T>(
  fn: () => Promise<T>,
  options: {
    successMessage?: string;
    errorMessage?: string;
    showLoading?: boolean;
    retryOptions?: RetryOptions;
  } = {}
): Promise<T | null> => {
  const {
    successMessage,
    errorMessage = "Something went wrong. Please try again.",
    showLoading = true,
    retryOptions,
  } = options;

  try {
    if (showLoading && typeof global !== "undefined" && global.toast) {
      // Could show a loading toast here if needed
    }

    const result = retryOptions
      ? await retryWithBackoff(fn, retryOptions)
      : await fn();

    if (successMessage && typeof global !== "undefined" && global.toast) {
      global.toast.success(successMessage);
    }

    return result;
  } catch (error) {
    console.error("Async call failed:", error);

    if (typeof global !== "undefined" && global.toast) {
      global.toast.error(errorMessage, {
        label: "Retry",
        onPress: () => {
          // Retry the operation
          safeAsyncCall(fn, options);
        },
      });
    }

    return null;
  }
};
