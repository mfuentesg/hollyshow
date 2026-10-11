import type { Timer } from "@hollyshow/protocol";
import { describe, expect, it } from "vitest";
import { timerDisplaySecs, timerElapsedMs, timerOvertime } from "./timers";

const base: Timer = {
  id: "t",
  label: "x",
  kind: "countdown",
  durationSecs: 60,
  run: "stopped",
  startedAtMs: null,
  elapsedMs: 0,
};

describe("timers", () => {
  it("stopped countdown shows its full duration", () => {
    expect(timerDisplaySecs(base, 5000)).toBe(60);
  });
  it("running countdown counts down and then overtime", () => {
    const t: Timer = { ...base, run: "running", startedAtMs: 1000, elapsedMs: 10_000 };
    expect(timerElapsedMs(t, 6000)).toBe(15_000);
    expect(timerDisplaySecs(t, 6000)).toBe(45);
    expect(timerOvertime(t, 6000)).toBe(false);
    expect(timerDisplaySecs(t, 1000 + 55_000)).toBe(-5);
    expect(timerOvertime(t, 1000 + 55_000)).toBe(true);
  });
  it("paused keeps accumulated time; stopwatch counts up", () => {
    const p: Timer = { ...base, run: "paused", elapsedMs: 20_000 };
    expect(timerDisplaySecs(p, 99999)).toBe(40);
    const sw: Timer = { ...base, kind: "stopwatch", run: "running", startedAtMs: 0, elapsedMs: 0 };
    expect(timerDisplaySecs(sw, 3500)).toBe(3);
  });
});
