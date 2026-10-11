import { describe, expect, it } from "vitest";
import { transposeChord, transposeKey, transposeText } from "./chords";
import { countSlides, sectionCodes, splitLyrics, suggestSections } from "./songText";

describe("splitLyrics", () => {
  it("--- and blank lines force breaks", () => {
    expect(splitLyrics("a\nb\n---\nc\nd", 4)).toEqual([
      ["a", "b"],
      ["c", "d"],
    ]);
    expect(splitLyrics("a\n\nb", 4)).toEqual([["a"], ["b"]]);
  });
  it("balances long blocks", () => {
    expect(splitLyrics("1\n2\n3\n4\n5", 4).map((b) => b.length)).toEqual([3, 2]);
    expect(countSlides("1\n2\n3\n4", 2)).toBe(2);
  });
  it("copes with CRLF and empty text", () => {
    expect(splitLyrics("a\r\nb", 4)).toEqual([["a", "b"]]);
    expect(splitLyrics("", 4)).toEqual([]);
  });
});

describe("suggestSections", () => {
  it("uses headings and repeated blocks", () => {
    const text =
      "Verso 1\nlinea uno\nlinea dos\n\nCoro:\ncoro a\ncoro b\n\nVerso 2\notra uno\notra dos\n\ncoro a\ncoro b";
    const { sections, sequence } = suggestSections(text);
    expect(sections.map((s) => s.kind)).toEqual(["verse", "chorus", "verse"]);
    expect(sequence).toHaveLength(4);
    expect(sequence[3]).toBe(sequence[1]);
  });
  it("detects an unlabeled repeated chorus", () => {
    const { sections } = suggestSections(
      "uno\ndos\n\nestribillo x\nestribillo y\n\ntres\ncuatro\n\nestribillo x\nestribillo y",
    );
    expect(sections.map((s) => s.kind)).toEqual(["verse", "chorus", "verse"]);
  });
});

describe("section codes", () => {
  it("numbers verses and repeated kinds", () => {
    const mk = (id: string, kind: "verse" | "chorus") => ({
      id,
      kind,
      label: id,
      lyrics: "",
      translation: null,
    });
    const codes = sectionCodes([mk("a", "verse"), mk("b", "chorus"), mk("c", "verse")]);
    expect([...codes.values()]).toEqual(["V1", "C", "V2"]);
  });
});

describe("chords", () => {
  it("transposes symbols and keeps qualities", () => {
    expect(transposeChord("G", 2)).toBe("A");
    expect(transposeChord("F#m7/C#", 1)).toBe("Gm7/D");
    expect(transposeChord("Bb", 2, true)).toBe("C");
    expect(transposeChord("N.C.", 3)).toBe("N.C.");
  });
  it("transposes text and keys", () => {
    expect(transposeText("[G]Sublime [D]gracia", 2)).toBe("[A]Sublime [E]gracia");
    expect(transposeText("x", 12)).toBe("x");
    expect(transposeKey("D", -2)).toBe("C");
    expect(transposeKey(null, 3)).toBeNull();
  });
});
