import { lyricSlide, sampleShow, type Cue, type Timer } from "@hollyshow/protocol";
import { describe, expect, it } from "vitest";
import {
  arrangementStrip,
  formatClock,
  formatDuration,
  nextCue,
  primaryCountdown,
  stageRows,
  timerElapsedMs,
} from "./stageModel";

const show = sampleShow();
const cue = (itemId: string, slideIndex: number): Cue => ({
  itemId,
  slideIndex,
  slide: show.find((e) => e.itemId === itemId)!.slides[slideIndex]!,
});
const timer = (patch: Partial<Timer>): Timer => ({
  id: "t",
  label: "Predicación",
  kind: "countdown",
  durationSecs: 600,
  run: "running",
  startedAtMs: 1000,
  elapsedMs: 0,
  ...patch,
});

describe("formatting", () => {
  it("clock is HH:MM", () => {
    expect(formatClock(new Date(2026, 0, 1, 9, 5).getTime())).toBe("09:05");
  });
  it("durations round up and support hours", () => {
    expect(formatDuration(192_000)).toBe("3:12");
    expect(formatDuration(1)).toBe("0:01");
    expect(formatDuration(-5)).toBe("0:00");
    expect(formatDuration(3_725_000)).toBe("1:02:05");
  });
});

describe("primaryCountdown", () => {
  it("prefers the item clock and flags urgency", () => {
    const clock = { itemId: "item-1", plannedSecs: 240, startedAtMs: 0 };
    expect(primaryCountdown(clock, [timer({})], 10_000)?.source).toBe("item");
    expect(primaryCountdown(clock, [], 10_000)?.urgency).toBe("ok");
    expect(primaryCountdown(clock, [], 200_000)?.urgency).toBe("warn");
    const over = primaryCountdown(clock, [], 250_000)!;
    expect(over.urgency).toBe("over");
    expect(over.ms).toBe(-10_000);
  });
  it("falls back to a running timer, then a paused one", () => {
    const c = primaryCountdown(
      null,
      [timer({ run: "paused", elapsedMs: 5000 }), timer({})],
      11_000,
    )!;
    expect(c.source).toBe("timer");
    expect(c.ms).toBe(600_000 - 10_000); // the running one
    expect(primaryCountdown(null, [timer({ run: "stopped" })], 0)).toBeNull();
    expect(primaryCountdown(null, [], 0)).toBeNull();
  });
  it("stopwatches count up", () => {
    const c = primaryCountdown(null, [timer({ kind: "stopwatch", elapsedMs: 2000 })], 4000)!;
    expect(c.stopwatch).toBe(true);
    expect(c.ms).toBe(2000 + 3000);
  });
  it("timer elapsed handles pause and stop", () => {
    expect(timerElapsedMs(timer({ run: "paused", elapsedMs: 700 }), 99_999)).toBe(700);
    expect(timerElapsedMs(timer({ run: "stopped", elapsedMs: 700 }), 99_999)).toBe(0);
  });
});

describe("stageRows", () => {
  const slide = { ...lyricSlide("s", "V1", ["Santo santo"]), chart: ["[D]Santo [G]santo"] };
  it("shows chords above the lyric only when enabled", () => {
    expect(stageRows(slide, true)).toEqual([{ chords: "D     G", lyric: "Santo santo" }]);
    expect(stageRows(slide, false)).toEqual([{ chords: null, lyric: "Santo santo" }]);
  });
  it("slides without a chart fall back to plain lines", () => {
    expect(stageRows(lyricSlide("s", null, ["a", "b"]), true).map((r) => r.lyric)).toEqual([
      "a",
      "b",
    ]);
  });
  it("Bible slides show the first version and its reference", () => {
    const b = {
      ...lyricSlide("b", null, []),
      kind: "bible" as const,
      bible: [{ version: "KJV", reference: "John 3:16", text: "For God", copyright: null }],
    };
    expect(stageRows(b, true).map((r) => r.lyric)).toEqual(["For God", "— John 3:16 (KJV)"]);
  });
});

describe("arrangement strip and next", () => {
  it("marks live and next keys", () => {
    const strip = arrangementStrip(show, cue("item-1", 0), cue("item-1", 1));
    expect(strip.map((k) => k.key)).toEqual(["V1", "C", "V2"]);
    expect(strip.map((k) => k.state)).toEqual(["live", "next", "idle"]);
  });
  it("next from another item does not mark the strip", () => {
    const strip = arrangementStrip(show, cue("item-1", 2), cue("item-2", 0));
    expect(strip.some((k) => k.state === "next")).toBe(false);
  });
  it("windows long items around the live slide", () => {
    const long = [
      {
        ...show[0]!,
        slides: Array.from({ length: 30 }, (_, i) => lyricSlide(`l${i}`, `Verso ${i}`, ["x"])),
      },
    ];
    const c: Cue = { itemId: "item-1", slideIndex: 15, slide: long[0]!.slides[15]! };
    const strip = arrangementStrip(long, c, null, 10);
    expect(strip).toHaveLength(10);
    expect(strip.some((k) => k.state === "live")).toBe(true);
    expect(arrangementStrip(long, null, null)).toEqual([]);
  });
  it("next prefers the preview, else the following slide across items", () => {
    expect(nextCue(show, cue("item-1", 0), cue("item-2", 0))?.slide.id).toBe("item-2:0");
    expect(nextCue(show, cue("item-1", 2), null)?.slide.id).toBe("item-2:0");
    expect(nextCue(show, cue("item-2", 0), null)).toBeNull();
  });
});
