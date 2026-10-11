import { describe, expect, it } from "vitest";
import { feedSectionKey, findSectionSlide, sectionKey } from "./sections";
import { lyricSlide } from "@hollyshow/protocol";

describe("section keys", () => {
  it.each([
    ["Verso 1", "V1"],
    ["Verso 12", "V12"],
    ["Verse 2", "V2"],
    ["Coro", "C"],
    ["Pre-coro", "P"],
    ["Puente", "U"],
    ["Final", "F"],
    ["Intro", "I"],
    ["Estribillo", "C"],
  ])("maps %s to %s", (label, key) => expect(sectionKey(label)).toBe(key));

  it("returns null for unlabeled or unknown", () => {
    expect(sectionKey(null)).toBeNull();
    expect(sectionKey("Juan 3:16")).toBeNull();
  });

  it("finds the first slide of a section", () => {
    const slides = [
      lyricSlide("a", "Verso 1", ["x"]),
      lyricSlide("b", "Coro", ["x"]),
      lyricSlide("c", "Coro", ["y"]),
    ];
    expect(findSectionSlide(slides, "c")).toBe(1);
    expect(findSectionSlide(slides, "V2")).toBe(-1);
  });
});

describe("section key buffer", () => {
  it("V then digit completes V<digit>", () => {
    const a = feedSectionKey({ buffer: "", at: 0 }, "v", 1000);
    expect(a.complete).toBeNull();
    const b = feedSectionKey(a.next, "2", 1300);
    expect(b.complete).toBe("V2");
  });
  it("letters complete immediately and the buffer expires", () => {
    expect(feedSectionKey({ buffer: "", at: 0 }, "c", 10).complete).toBe("C");
    const a = feedSectionKey({ buffer: "", at: 0 }, "v", 1000);
    expect(feedSectionKey(a.next, "2", 3000).complete).toBe("V2"); // bare digit also jumps to verse
  });
  it("ignores other keys", () => {
    expect(feedSectionKey({ buffer: "", at: 0 }, "x", 10).complete).toBeNull();
  });
});
