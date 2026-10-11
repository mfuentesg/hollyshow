export interface BackoffOptions {
  baseMs?: number;
  maxMs?: number;
  /** Fraction of the delay randomised (0.3 => +-30%). Avoids a herd of phones reconnecting together. */
  jitter?: number;
}

/** Exponential reconnect delay: 0.5 s, 1 s, 2 s ... capped at 15 s. `rand` is injectable for tests. */
export function backoffDelay(
  attempt: number,
  opts: BackoffOptions = {},
  rand: () => number = Math.random,
): number {
  const { baseMs = 500, maxMs = 15_000, jitter = 0.3 } = opts;
  const raw = Math.min(maxMs, baseMs * 2 ** Math.max(0, Math.min(attempt, 30)));
  const spread = raw * jitter * (rand() * 2 - 1);
  return Math.max(0, Math.round(raw + spread));
}
