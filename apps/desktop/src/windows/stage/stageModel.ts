import type { Cue, ItemClock, ShowEntry, Slide, Timer } from "@hollyshow/protocol";
import { parseChordLine, sectionKey } from "@hollyshow/slide-renderer";

/** "10:11" (24 h, no seconds) for the big clock. */
export function formatClock(ms: number): string {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** "3:12" / "1:02:09" from a non-negative millisecond count (rounded up so 0:00 is really zero). */
export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
    : `${m}:${String(s).padStart(2, "0")}`;
}

export type Urgency = "ok" | "warn" | "over";

export interface Countdown {
  source: "item" | "timer";
  label: string | null;
  /** Remaining ms (negative once overrun) for countdowns; elapsed ms for stopwatches. */
  ms: number;
  stopwatch: boolean;
  urgency: Urgency;
}

const WARN_MS = 60_000;

function urgencyOf(remainingMs: number): Urgency {
  return remainingMs < 0 ? "over" : remainingMs <= WARN_MS ? "warn" : "ok";
}

export function timerElapsedMs(t: Timer, nowMs: number): number {
  const running =
    t.run === "running" && t.startedAtMs !== null ? Math.max(0, nowMs - t.startedAtMs) : 0;
  return t.run === "stopped" ? 0 : t.elapsedMs + running;
}

/** The main countdown of the stage: the live item's clock, else the first running timer. */
export function primaryCountdown(
  itemClock: ItemClock | null,
  timers: Timer[],
  nowMs: number,
): Countdown | null {
  if (itemClock) {
    const remaining = itemClock.plannedSecs * 1000 - Math.max(0, nowMs - itemClock.startedAtMs);
    return {
      source: "item",
      label: null,
      ms: remaining,
      stopwatch: false,
      urgency: urgencyOf(remaining),
    };
  }
  const t = timers.find((x) => x.run === "running") ?? timers.find((x) => x.run === "paused");
  if (!t) return null;
  const elapsed = timerElapsedMs(t, nowMs);
  if (t.kind === "stopwatch") {
    return { source: "timer", label: t.label, ms: elapsed, stopwatch: true, urgency: "ok" };
  }
  const remaining = t.durationSecs * 1000 - elapsed;
  return {
    source: "timer",
    label: t.label,
    ms: remaining,
    stopwatch: false,
    urgency: urgencyOf(remaining),
  };
}

/** Lines the stage shows for a slide (Bible: first version plus its reference). */
export function stageLines(slide: Slide): string[] {
  const first = slide.bible[0];
  if (first) return [first.text, `— ${first.reference} (${first.version})`];
  return slide.lines;
}

export interface StageRow {
  chords: string | null;
  lyric: string;
}

/** Rows for the current slide: with `chords`, chart lines produce a chord row above the lyric. */
export function stageRows(slide: Slide, chords: boolean): StageRow[] {
  if (chords && slide.chart.length > 0) {
    return slide.chart.flatMap((line): StageRow[] => {
      const p = parseChordLine(line);
      return [{ chords: p.hasChords ? p.chords : null, lyric: p.lyric }];
    });
  }
  return stageLines(slide).map((lyric) => ({ chords: null, lyric }));
}

export interface StripKey {
  key: string;
  state: "live" | "next" | "idle";
  index: number;
}

/**
 * Arrangement strip: one key per slide of the live item (slides already follow the arrangement,
 * repeats included). The live slide is red, the one shown as "next" is outlined in blue. Long
 * items are windowed around the live slide so the strip never overflows.
 */
export function arrangementStrip(
  show: ShowEntry[],
  live: Cue | null,
  next: Cue | null,
  max = 10,
): StripKey[] {
  if (!live?.itemId) return [];
  const entry = show.find((e) => e.itemId === live.itemId);
  if (!entry) return [];
  const nextIndex = next && next.itemId === live.itemId ? next.slideIndex : -1;
  const all = entry.slides.map<StripKey>((s, index) => ({
    key: sectionKey(s.label),
    index,
    state: index === live.slideIndex ? "live" : index === nextIndex ? "next" : "idle",
  }));
  if (all.length <= max) return all;
  const start = Math.min(Math.max(0, live.slideIndex - 2), all.length - max);
  return all.slice(start, start + max);
}

/** "Canción", "Biblia"... translated by the caller through the returned key. */
export function kindKey(slide: Slide): "song" | "bible" | "text" | "media" | "other" {
  switch (slide.kind) {
    case "lyrics":
      return "song";
    case "bible":
      return "bible";
    case "text":
      return "text";
    case "media":
      return "media";
    default:
      return "other";
  }
}

/**
 * The slide shown as "next": the staged preview when there is one (Escenario receives it for that
 * purpose), otherwise the following slide in show order.
 */
export function nextCue(show: ShowEntry[], live: Cue | null, preview: Cue | null): Cue | null {
  if (preview) return preview;
  if (!live?.itemId) return null;
  const i = show.findIndex((e) => e.itemId === live.itemId);
  const entry = show[i];
  if (!entry) return null;
  const inItem = entry.slides[live.slideIndex + 1];
  if (inItem) return { itemId: entry.itemId, slideIndex: live.slideIndex + 1, slide: inItem };
  const nextEntry = show[i + 1];
  const first = nextEntry?.slides[0];
  return nextEntry && first ? { itemId: nextEntry.itemId, slideIndex: 0, slide: first } : null;
}
