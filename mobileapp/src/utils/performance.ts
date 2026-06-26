/**
 * Lightweight performance timing utility.
 * All logging is gated behind __DEV__ — zero overhead in production.
 */

const marks = new Map<string, number>();

export function markStart(label: string): void {
  if (!__DEV__) return;
  marks.set(label, Date.now());
}

export function markEnd(label: string): void {
  if (!__DEV__) return;
  const start = marks.get(label);
  if (start == null) return;
  console.log(`[perf] ${label}: ${Date.now() - start}ms`);
  marks.delete(label);
}

/** Call on navigation state change to log screen transition time. */
export function logNavigation(routeName: string): void {
  if (!__DEV__) return;
  markEnd(`nav:${routeName}`);
}

export function startNavigation(routeName: string): void {
  if (!__DEV__) return;
  markStart(`nav:${routeName}`);
}
