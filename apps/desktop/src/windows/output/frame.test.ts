import { emptyState, lyricSlide, sampleShow, type StateView } from "@hollyshow/protocol";
import { describe, expect, it } from "vitest";
import { absoluteIndex, directionBetween, frameFor, nextSlideAfter, positionLabel } from "./frame";

const show = sampleShow();
const cue = (itemId: string, slideIndex: number) => {
  const slide = show.find((e) => e.itemId === itemId)!.slides[slideIndex]!;
  return { itemId, slideIndex, slide };
};
function view(patch: Partial<StateView> = {}): StateView {
  const { show: _s, ...v } = emptyState();
  return { ...v, live: cue("item-1", 0), ...patch };
}

describe("frameFor (mirror of StateView::frame_for)", () => {
  it("is null for unknown and disabled outputs", () => {
    expect(frameFor(view(), "nope")).toBeNull();
    expect(frameFor(view(), "stage")).toBeNull(); // disabled in emptyState
  });
  it("global blackout applies to outputs that follow it", () => {
    const f = frameFor(view({ blackout: true, logo: true }), "main")!;
    expect(f.blackout).toBe(true);
    expect(f.logo).toBe(false);
    expect(f.showText).toBe(false);
  });
  it("outputs that ignore blackout stay visible", () => {
    const v = view({ blackout: true });
    v.outputs = { ...v.outputs, main: { ...v.outputs.main!, followsBlackout: false } };
    expect(frameFor(v, "main")!.blackout).toBe(false);
  });
  it("per-output blackout does not need the global flag", () => {
    const v = view();
    v.outputLayers = { main: { ...v.outputLayers.main!, blackout: true } };
    expect(frameFor(v, "main")!.blackout).toBe(true);
  });
  it("clear and the text layer hide the text but not the cue", () => {
    const f = frameFor(view({ clear: true }), "main")!;
    expect(f.showText).toBe(false);
    expect(f.cue).not.toBeNull();
    const v = view();
    v.outputLayers = { main: { ...v.outputLayers.main!, text: false, overlay: "aviso" } };
    const g = frameFor(v, "main")!;
    expect(g.showText).toBe(false);
    expect(g.overlay).toBe("aviso");
  });
  it("overlay is dropped under blackout", () => {
    const v = view({ blackout: true });
    v.outputLayers = { main: { ...v.outputLayers.main!, overlay: "x" } };
    expect(frameFor(v, "main")!.overlay).toBeNull();
  });
});

describe("show navigation helpers", () => {
  it("next slide crosses item boundaries", () => {
    expect(nextSlideAfter(show, cue("item-1", 0))?.id).toBe("item-1:1");
    expect(nextSlideAfter(show, cue("item-1", 2))?.id).toBe("item-2:0");
    expect(nextSlideAfter(show, cue("item-2", 0))).toBeNull();
    expect(
      nextSlideAfter(show, { itemId: null, slideIndex: 0, slide: lyricSlide("x", null, []) }),
    ).toBeNull();
  });
  it("absolute index and direction", () => {
    expect(absoluteIndex(show, cue("item-1", 1))).toBe(1);
    expect(absoluteIndex(show, cue("item-2", 0))).toBe(3);
    expect(directionBetween(3, 1)).toBe("back");
    expect(directionBetween(1, 3)).toBe("forward");
    expect(directionBetween(null, 3)).toBe("forward");
  });
  it("position label", () => {
    expect(positionLabel(show, cue("item-1", 1))).toBe("C · 2/3");
    expect(positionLabel(show, cue("item-2", 0))).toBe("1/1");
    expect(positionLabel(show, null)).toBeUndefined();
  });
});
