import { describe, expect, it } from "vitest";
import { applyMarks } from "./marks";

describe("applyMarks", () => {
  it("returns one plain span without marks", () => {
    expect(applyMarks("hola", [])).toEqual([{ text: "hola", jesus: false, italic: false }]);
  });
  it("splits around a words-of-Jesus mark using scalar offsets", () => {
    const spans = applyMarks("Él dijo: Venid", [{ start: 9, end: 14, kind: "wordsOfJesus" }]);
    expect(spans.map((s) => [s.text, s.jesus])).toEqual([
      ["Él dijo: ", false],
      ["Venid", true],
    ]);
  });
  it("handles overlapping kinds and out-of-range marks", () => {
    const spans = applyMarks("abcdef", [
      { start: 0, end: 4, kind: "italic" },
      { start: 2, end: 99, kind: "wordsOfJesus" },
    ]);
    expect(spans.map((s) => [s.text, s.italic, s.jesus])).toEqual([
      ["ab", true, false],
      ["cd", true, true],
      ["ef", false, true],
    ]);
  });
});
