import { emptyState, lyricSlide, sampleShow, type Timer } from "@hollyshow/protocol";
import { describe, expect, it } from "vitest";
import {
  findEntry,
  formatClock,
  itemClockMs,
  pickColumns,
  slideKey,
  slideText,
  timerValueMs,
} from "./slideText";

const timer = (over: Partial<Timer>): Timer => ({
  id: "t1",
  label: "Predica",
  kind: "countdown",
  durationSecs: 600,
  run: "stopped",
  startedAtMs: null,
  elapsedMs: 0,
  ...over,
});

describe("slideText", () => {
  it("joins lyric lines and bible columns", () => {
    expect(slideText(lyricSlide("a", null, ["uno", "dos"]))).toBe("uno\ndos");
    const bible = {
      ...lyricSlide("b", null, []),
      kind: "bible" as const,
      bible: [
        {
          version: "RVR1909",
          reference: "Juan 3:16",
          text: "Porque de tal manera",
          copyright: null,
        },
        { version: "KJV", reference: "John 3:16", text: "For God so loved", copyright: null },
      ],
    };
    expect(slideText(bible)).toBe("Porque de tal manera\n\nFor God so loved");
    expect(slideText(null)).toBe("");
  });
  it("labels keys like the mockup", () => {
    expect(slideKey("Verso 1")).toBe("V1");
    expect(slideKey("Coro")).toBe("C");
    expect(slideKey(null)).toBe("·");
  });
  it("finds the entry of a cue", () => {
    const show = sampleShow();
    const cue = { itemId: "item-2", slideIndex: 0, slide: lyricSlide("x", null, []) };
    expect(findEntry(show, cue)?.title).toBe("Anuncios");
    expect(findEntry(show, { ...cue, itemId: null })).toBeNull();
    expect(findEntry(show, null)).toBeNull();
  });
  it("picks bible columns by version with fallback", () => {
    const cols = [
      { version: "A", reference: "", text: "a", copyright: null },
      { version: "B", reference: "", text: "b", copyright: null },
    ];
    expect(pickColumns(cols, "B").map((c) => c.text)).toEqual(["b"]);
    expect(pickColumns(cols, "Z")).toHaveLength(2);
    expect(pickColumns(cols, null)).toHaveLength(2);
  });
});

describe("timers", () => {
  it("counts down a running timer from the host clock", () => {
    const t = timer({ run: "running", startedAtMs: 1_000, elapsedMs: 10_000 });
    expect(timerValueMs(t, 6_000)).toBe(600_000 - 15_000);
  });
  it("goes negative on overtime and ignores the clock while paused", () => {
    expect(timerValueMs(timer({ run: "running", startedAtMs: 0 }), 601_000)).toBe(-1_000);
    expect(timerValueMs(timer({ run: "paused", elapsedMs: 5_000, startedAtMs: 0 }), 99_999)).toBe(
      595_000,
    );
    expect(timerValueMs(timer({}), 99_999)).toBe(600_000);
  });
  it("counts a stopwatch up", () => {
    expect(
      timerValueMs(
        timer({ kind: "stopwatch", run: "running", startedAtMs: 0, elapsedMs: 1_000 }),
        2_000,
      ),
    ).toBe(3_000);
  });
  it("reads the item clock", () => {
    const view = {
      ...emptyState(),
      itemClock: { itemId: "i", plannedSecs: 60, startedAtMs: 1_000 },
    };
    expect(itemClockMs(view, 31_000)).toBe(30_000);
    expect(itemClockMs(view, 71_000)).toBe(-10_000);
    expect(itemClockMs(emptyState(), 0)).toBeNull();
  });
  it("formats clocks", () => {
    expect(formatClock(247_000)).toBe("04:07");
    expect(formatClock(-12_000)).toBe("-00:12");
    expect(formatClock(3_723_000)).toBe("1:02:03");
  });
});
