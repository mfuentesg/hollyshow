import { describe, expect, it } from "vitest";
import { buildTransition, cutPlan, lineRevealDelays, playTransition } from "./transitions";

const only = (frames: Keyframe[] | null) =>
  new Set((frames ?? []).flatMap((f) => Object.keys(f).filter((k) => k !== "offset")));

describe("buildTransition", () => {
  it("cut and zero duration produce no animation", () => {
    expect(buildTransition({ kind: "cut", durationMs: 300 })).toBe(cutPlan);
    expect(buildTransition({ kind: "fade", durationMs: 0 })).toBe(cutPlan);
  });
  it("only ever animates opacity and transform (compositor properties)", () => {
    for (const kind of ["fade", "crossfade", "slide", "zoom", "blur", "shader"] as const) {
      const p = buildTransition({ kind, durationMs: 400 }, "forward");
      for (const k of [...only(p.incoming), ...only(p.outgoing)]) {
        expect(["opacity", "transform"]).toContain(k);
      }
    }
  });
  it("shader degrades to a crossfade on the DOM layer", () => {
    const a = buildTransition({ kind: "shader", durationMs: 400 });
    const b = buildTransition({ kind: "crossfade", durationMs: 400 });
    expect(a).toEqual(b);
  });
  it("fade goes through the background: incoming waits for outgoing", () => {
    const p = buildTransition({ kind: "fade", durationMs: 400 });
    expect(p.durationMs).toBe(200);
    expect(p.incomingDelayMs).toBe(200);
  });
  it("slide mirrors with direction", () => {
    const f = buildTransition({ kind: "slide", durationMs: 300 }, "forward");
    const b = buildTransition({ kind: "slide", durationMs: 300 }, "back");
    expect(String(f.incoming?.[0]?.transform)).toContain("6%");
    expect(String(b.incoming?.[0]?.transform)).toContain("-6%");
  });
  it("clamps absurd durations", () => {
    expect(buildTransition({ kind: "crossfade", durationMs: 99999 }).durationMs).toBe(5000);
  });
});

describe("lineRevealDelays", () => {
  it("staggers lines with a bounded step", () => {
    const d = lineRevealDelays(4, 1200);
    expect(d[0]).toBe(0);
    expect(d).toEqual([...d].sort((a, b) => a - b));
    expect(new Set(d).size).toBe(4);
    expect(lineRevealDelays(0, 500)).toEqual([]);
    expect(lineRevealDelays(1, 500)).toEqual([0]);
    expect(Math.max(...lineRevealDelays(3, 1_000_000))).toBeLessThanOrEqual(700);
  });
});

describe("playTransition", () => {
  function fakeEl() {
    const calls: Array<{ keyframes: Keyframe[]; opts: KeyframeAnimationOptions }> = [];
    const el = {
      calls,
      animate(keyframes: Keyframe[], opts: KeyframeAnimationOptions) {
        calls.push({ keyframes, opts });
        return { finished: Promise.resolve(), cancel() {} } as unknown as Animation;
      },
    };
    return el as unknown as Element & { calls: typeof calls };
  }
  it("animates both layers and honours the incoming delay", async () => {
    const out = fakeEl();
    const inc = fakeEl();
    const plan = buildTransition({ kind: "fade", durationMs: 400 });
    const h = playTransition(out, inc, { plan, lineByLine: false, lines: [] });
    await h.finished;
    expect(out.calls).toHaveLength(1);
    expect(inc.calls[0]?.opts.delay).toBe(200);
  });
  it("reveals line by line and fades the old text quickly", () => {
    const out = fakeEl();
    const inc = fakeEl();
    const l1 = fakeEl();
    const l2 = fakeEl();
    const plan = buildTransition({ kind: "crossfade", durationMs: 400 });
    playTransition(out, inc, { plan, lineByLine: true, lines: [l1, l2] });
    expect(inc.calls).toHaveLength(0);
    expect(l1.calls).toHaveLength(1);
    expect((l2.calls[0]?.opts.delay as number) > (l1.calls[0]?.opts.delay as number)).toBe(true);
    expect(out.calls[0]?.opts.duration).toBeLessThanOrEqual(200);
  });
  it("reduced motion is a cut", () => {
    const out = fakeEl();
    const inc = fakeEl();
    playTransition(out, inc, {
      plan: buildTransition({ kind: "zoom", durationMs: 400 }),
      lineByLine: false,
      lines: [],
      reducedMotion: true,
    });
    expect(out.calls).toHaveLength(0);
    expect(inc.calls).toHaveLength(0);
  });
});
