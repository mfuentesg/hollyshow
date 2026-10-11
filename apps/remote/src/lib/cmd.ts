import type { Command, TimerId } from "@hollyshow/protocol";

/** Builders missing from `@hollyshow/protocol`'s `commands` (kept local; protocol is not ours). */
export const timerCmd = {
  start: (id: TimerId): Command => ({ kind: "startTimer", id }),
  pause: (id: TimerId): Command => ({ kind: "pauseTimer", id }),
  reset: (id: TimerId): Command => ({ kind: "resetTimer", id }),
};
