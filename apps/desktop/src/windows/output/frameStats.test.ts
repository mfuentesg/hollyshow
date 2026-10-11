import { describe, expect, it } from "vitest";
import { FrameStats } from "./frameStats";

describe("FrameStats", () => {
  it("reports about 60 fps for a steady 16.7 ms cadence", () => {
    const s = new FrameStats();
    let out = null;
    for (let t = 0; t <= 2100 && !out; t += 1000 / 60) out = s.tick(t);
    expect(out).not.toBeNull();
    expect(out!.fps).toBeGreaterThan(58);
    expect(out!.fps).toBeLessThan(62);
    expect(out!.dropped).toBe(0);
  });

  it("counts slow frames as dropped", () => {
    const s = new FrameStats();
    s.tick(0);
    s.tick(16);
    s.tick(130); // a 114 ms hitch
    s.tick(146);
    const out = s.tick(2100);
    expect(out?.dropped).toBe(2); // the hitch and the long gap before 2100
  });

  it("stays quiet until the period elapsed", () => {
    const s = new FrameStats();
    expect(s.tick(0)).toBeNull();
    expect(s.tick(1000)).toBeNull();
  });
});
