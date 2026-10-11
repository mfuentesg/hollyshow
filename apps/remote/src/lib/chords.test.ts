import { describe, expect, it } from "vitest";
import { hasChords, parseChart, semitoneLabel, transposeChart, transposeChord } from "./chords";

describe("parseChart", () => {
  it("splits chords over the text that follows them", () => {
    expect(parseChart("[D]Santo, [G]santo")).toEqual([
      { chord: "D", text: "Santo, " },
      { chord: "G", text: "santo" },
    ]);
  });
  it("keeps text before the first chord and trailing chords", () => {
    expect(parseChart("Oh [Am]Dios[C]")).toEqual([
      { chord: null, text: "Oh " },
      { chord: "Am", text: "Dios" },
      { chord: "C", text: "" },
    ]);
  });
  it("returns plain lines untouched", () => {
    expect(parseChart("sin acordes")).toEqual([{ chord: null, text: "sin acordes" }]);
    expect(hasChords("sin acordes")).toBe(false);
    expect(parseChart("")).toEqual([]);
  });
});

describe("transposeChord", () => {
  it("moves roots and keeps the quality", () => {
    expect(transposeChord("D", 2)).toBe("E");
    expect(transposeChord("F#m7", 1)).toBe("Gm7");
    expect(transposeChord("Am(add9)", 3)).toBe("Cm(add9)");
    expect(transposeChord("B", 1)).toBe("C");
  });
  it("wraps in both directions", () => {
    expect(transposeChord("C", -1)).toBe("B");
    expect(transposeChord("A", 14)).toBe("B");
  });
  it("keeps flats as flats and transposes the bass", () => {
    expect(transposeChord("Bb", 2)).toBe("C");
    expect(transposeChord("Eb", 1)).toBe("E");
    expect(transposeChord("Bb/D", 1)).toBe("B/Eb");
    expect(transposeChord("G/B", 2)).toBe("A/C#");
  });
  it("supports Latin names", () => {
    expect(transposeChord("Sol", 2)).toBe("La");
    expect(transposeChord("Rem", 2)).toBe("Mim");
    expect(transposeChord("Do#", 1)).toBe("Re");
  });
  it("ignores things that are not chords and zero shifts", () => {
    expect(transposeChord("N.C.", 3)).toBe("N.C.");
    expect(transposeChord("", 3)).toBe("");
    expect(transposeChord("G", 0)).toBe("G");
    expect(transposeChord("G", 12)).toBe("G");
  });
});

describe("transposeChart", () => {
  it("transposes every chord of a line", () => {
    expect(transposeChart("[G]Sublime [D]gracia", 2)).toBe("[A]Sublime [E]gracia");
  });
  it("leaves lines without chords alone", () => {
    expect(transposeChart("Hola", 5)).toBe("Hola");
  });
});

describe("semitoneLabel", () => {
  it("signs the value", () => {
    expect(semitoneLabel(2)).toBe("+2");
    expect(semitoneLabel(-1)).toBe("-1");
    expect(semitoneLabel(0)).toBe("0");
  });
});
