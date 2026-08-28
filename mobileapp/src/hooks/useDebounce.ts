import { useEffect, useRef, useCallback } from "react";

export interface DebouncedFunction<T extends (...args: Parameters<T>) => ReturnType<T>> {
  (...args: Parameters<T>): void;
  cancel: () => void;
}

export function useDebounce<T extends (...args: Parameters<T>) => ReturnType<T>>(
  callback: T,
  delay: number
): DebouncedFunction<T> {
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
        timeoutRef.current = null;
      }
    };
  }, []);

  const debouncedFn = useCallback(
    (...args: Parameters<T>) => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }

      timeoutRef.current = setTimeout(() => {
        callback(...args);
      }, delay);
    },
    [callback, delay]
  );

  const cancel = useCallback(() => {
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    }
  }, []);

  return Object.assign(debouncedFn, { cancel }) as DebouncedFunction<T>;
}
