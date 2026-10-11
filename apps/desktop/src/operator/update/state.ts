import type { ReleaseInfo, UpdateEvent, UpdateState } from "./types";

/** Folds a backend event into the client state. Pure so it is trivial to test. */
export function applyEvent(prev: UpdateState, ev: UpdateEvent): UpdateState {
  switch (ev.type) {
    case "checking":
      // Do not hide a downloaded update behind a "checking" spinner.
      return prev.state === "ready" || prev.state === "downloading" ? prev : { state: "checking" };
    case "available":
      return prev.state === "ready" ? prev : { state: "available", ...ev.info };
    case "progress":
      if (prev.state === "ready") return prev;
      return {
        state: "downloading",
        ...(prev.state === "available" || prev.state === "downloading"
          ? { notes: prev.notes, date: prev.date }
          : {}),
        version: ev.version,
        downloaded: ev.downloaded,
        total: ev.total,
      };
    case "ready":
      return { state: "ready", deferred: ev.deferred, ...ev.info };
    case "none":
      return prev.state === "ready" || prev.state === "downloading" ? prev : { state: "idle" };
    case "error":
      return { state: "error", message: ev.message };
  }
}

/** Download percentage 0..100, or null when the total size is unknown. */
export function percent(downloaded: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.max(0, Math.min(100, Math.round((downloaded / total) * 100)));
}

export function releaseOf(s: UpdateState): ReleaseInfo | null {
  if (s.state === "ready" || s.state === "available") {
    return { version: s.version, notes: s.notes, date: s.date };
  }
  if (s.state === "downloading") {
    return { version: s.version, notes: s.notes ?? "", date: s.date ?? null };
  }
  return null;
}
