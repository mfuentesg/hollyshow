/** Accent- and case-insensitive folding used for every local search ("canción" matches "cancion"). */
export function fold(s: string): string {
  return s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** True when every whitespace-separated token of `query` appears in `haystack` (folded). */
export function matchesAll(haystack: string, query: string): boolean {
  const h = fold(haystack);
  return fold(query)
    .split(/\s+/)
    .filter(Boolean)
    .every((tok) => h.includes(tok));
}

export function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}

export function uid(prefix: string): string {
  const rnd =
    typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID().slice(0, 8)
      : Math.random().toString(36).slice(2, 10);
  return `${prefix}-${rnd}`;
}
