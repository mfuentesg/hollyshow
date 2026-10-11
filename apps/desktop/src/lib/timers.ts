import type { Timer } from "@hollyshow/protocol";

/** Elapsed time of a timer at `now` (host clock). Mirrors `Timer` semantics in crates/core. */
export function timerElapsedMs(t: Timer, now: number): number {
  const running =
    t.run === "running" && t.startedAtMs !== null ? Math.max(0, now - t.startedAtMs) : 0;
  return t.elapsedMs + running;
}

/** Signed seconds to display: remaining for countdowns (negative = overtime), elapsed for stopwatches. */
export function timerDisplaySecs(t: Timer, now: number): number {
  const elapsed = timerElapsedMs(t, now);
  return t.kind === "countdown"
    ? Math.ceil((t.durationSecs * 1000 - elapsed) / 1000)
    : Math.floor(elapsed / 1000);
}

export function timerOvertime(t: Timer, now: number): boolean {
  return t.kind === "countdown" && timerElapsedMs(t, now) > t.durationSecs * 1000;
}
