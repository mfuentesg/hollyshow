import { describe, expect, it } from "vitest";
import { parseChordLine, sectionKey } from "./chords";

describe("parseChordLine", () => {
  it("aligns chords over the syllable they precede", () => {
    const r = parseChordLine("[D]Santo, [G]santo");
    expect(r.lyric).toBe("Santo, santo");
    expect(r.chords).toBe("D      G");
    expect(r.chords.indexOf("G")).toBe(r.lyric.indexOf("santo"));
    expect(r.hasChords).toBe(true);
  });
  it("separates adjacent chords so they never fuse", () => {
    const r = parseChordLine("[D][G]Hola");
    expect(r.chords).toBe("D G");
  });
  it("handles trailing chords and plain lines", () => {
    expect(parseChordLine("Hola[A]").chords).toBe("    A");
    const plain = parseChordLine("sin acordes");
    expect(plain.hasChords).toBe(false);
    expect(plain.chords).toBe("");
  });
  it("keeps chord qualities", () => {
    expect(parseChordLine("[F#m7/C#]Dios").chords).toBe("F#m7/C#");
  });
});

describe("sectionKey", () => {
  it("abbreviates section labels", () => {
    expect(sectionKey("Verso 1")).toBe("V1");
    expect(sectionKey("Coro")).toBe("C");
    expect(sectionKey("Puente 2")).toBe("P2");
    expect(sectionKey(null)).toBe("·");
  });
});
