import type { ServiceItem } from "@hollyshow/protocol";

/** Seconds since midnight for "HH:MM" (invalid -> fallback). */
export function parseClock(hhmm: string, fallback = 10 * 3600): number {
  const m = /^(\d{1,2}):(\d{2})$/.exec(hhmm.trim());
  if (!m) return fallback;
  const h = +m[1]!;
  const mi = +m[2]!;
  return h < 24 && mi < 60 ? h * 3600 + mi * 60 : fallback;
}

/** "10:05" from seconds since midnight (wraps at 24 h). */
export function formatClock(secs: number): string {
  const s = ((Math.floor(secs) % 86400) + 86400) % 86400;
  return `${String(Math.floor(s / 3600)).padStart(2, "0")}:${String(Math.floor((s % 3600) / 60)).padStart(2, "0")}`;
}

/** "m:ss" or "h:mm:ss" for timers and clocks. */
export function formatDuration(totalSecs: number): string {
  const sign = totalSecs < 0 ? "-" : "";
  const s = Math.floor(Math.abs(totalSecs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${sign}${h}:${String(m).padStart(2, "0")}:${ss}` : `${sign}${m}:${ss}`;
}

export interface ScheduledItem {
  itemId: string;
  startSecs: number;
  durationSecs: number | null;
}

/** Start of each item = service start + sum of the planned durations before it. */
export function schedule(items: ServiceItem[], startSecs: number): ScheduledItem[] {
  let t = startSecs;
  return items.map((it) => {
    const row = { itemId: it.id, startSecs: t, durationSecs: it.durationSecs };
    t += it.durationSecs ?? 0;
    return row;
  });
}

export function totalDuration(items: ServiceItem[]): number {
  return items.reduce((a, it) => a + (it.durationSecs ?? 0), 0);
}
