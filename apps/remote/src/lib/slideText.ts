import type { BibleColumn, Cue, ShowEntry, Slide, StateView, Timer } from "@hollyshow/protocol";

/** Plain text of a slide for small previews. Bible slides list each version's text and reference. */
export function slideText(slide: Slide | null | undefined): string {
  if (!slide) return "";
  if (slide.kind === "bible" && slide.bible.length > 0) {
    return slide.bible.map((c) => c.text).join("\n\n");
  }
  return slide.lines.join("\n");
}

/** "Santo, santo" label key: "Verso 1" -> "V1", "Coro" -> "C", null -> "·". */
export function slideKey(label: string | null | undefined): string {
  if (!label) return "·";
  const words = label.trim().split(/\s+/);
  const first = words[0] ?? "";
  const num = words.find((w) => /^\d+$/.test(w));
  return `${first.charAt(0).toUpperCase()}${num ?? ""}` || "·";
}

export function findEntry(show: ShowEntry[], cue: Cue | null): ShowEntry | null {
  if (!cue || cue.itemId === null) return null;
  return show.find((e) => e.itemId === cue.itemId) ?? null;
}

/** Bible columns to show for a chosen version (all of them when the version is not on screen). */
export function pickColumns(columns: BibleColumn[], version: string | null): BibleColumn[] {
  if (!version) return columns;
  const hit = columns.filter((c) => c.version === version);
  return hit.length > 0 ? hit : columns;
}

/** Remaining milliseconds of a countdown (negative = overtime) or elapsed of a stopwatch. */
export function timerValueMs(t: Timer, serverNow: number): number {
  const running =
    t.run === "running" && t.startedAtMs !== null ? Math.max(0, serverNow - t.startedAtMs) : 0;
  const elapsed = t.run === "stopped" ? 0 : t.elapsedMs + running;
  return t.kind === "countdown" ? t.durationSecs * 1000 - elapsed : elapsed;
}

/** Remaining ms of the live item's planned duration (negative = overtime); null when there is no clock. */
export function itemClockMs(view: StateView, serverNow: number): number | null {
  const c = view.itemClock;
  if (!c) return null;
  return c.plannedSecs * 1000 - Math.max(0, serverNow - c.startedAtMs);
}

/** "04:07", "-00:12" (overtime) or "1:02:03". */
export function formatClock(ms: number): string {
  const neg = ms < 0;
  const total = Math.floor(Math.abs(ms) / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  const body = h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
  return neg ? `-${body}` : body;
}
