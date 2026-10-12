import { describe, expect, it } from "vitest";
import { createDemoBackend } from "./backend";
import { applyDemoPreset } from "./preset";

describe("applyDemoPreset", () => {
  it("puts the first Santo slide live and the second in preview with ?demo=live", async () => {
    const demo = createDemoBackend();
    await applyDemoPreset(demo.api, "?demo=live");
    expect(demo.state.live?.itemId).toBe("i-santo");
    expect(demo.state.live?.slideIndex).toBe(0);
    expect(demo.state.preview?.slideIndex).toBe(1);
  });

  it("does nothing without the flag", async () => {
    const demo = createDemoBackend();
    await applyDemoPreset(demo.api, "");
    expect(demo.state.live).toBeNull();
  });
});
