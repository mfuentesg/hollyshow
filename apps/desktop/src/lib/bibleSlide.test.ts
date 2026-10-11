import { describe, expect, it } from "vitest";
import { composeBibleSlide } from "./bibleSlide";

const verse = (n: number, text: string) => ({
  reference: { book: "John", chapter: 3, verse: n },
  text,
  marks: [],
});
const passage = {
  start: { book: "John", chapter: 3, verse: 16 },
  end: { book: "John", chapter: 3, verse: 17 },
};

describe("composeBibleSlide", () => {
  it("builds one column per version with localized references", () => {
    const s = composeBibleSlide(
      "x",
      passage,
      [
        {
          code: "RVR1909",
          language: "es",
          copyright: null,
          verses: [verse(16, "uno"), verse(17, "dos")],
        },
        {
          code: "KJV",
          language: "en",
          copyright: "PD",
          verses: [verse(16, "one"), verse(17, "two")],
        },
      ],
      "columns",
      { showCopyright: true, showVersionLabel: true },
    );
    expect(s.bible.map((c) => c.reference)).toEqual(["Juan 3:16–17", "John 3:16–17"]);
    expect(s.bible[0]!.text).toBe("16 uno 17 dos");
    expect(s.bible[1]!.copyright).toBe("PD");
    expect(s.footer).toBe("RVR1909 · KJV");
  });

  it("marks missing verses and hides copyright when asked", () => {
    const s = composeBibleSlide(
      "x",
      passage,
      [{ code: "A", language: "es", copyright: "c", verses: [] }],
      "stacked",
      {
        showCopyright: false,
        showVersionLabel: false,
      },
    );
    expect(s.bible[0]!.text).toBe("—");
    expect(s.bible[0]!.copyright).toBeNull();
    expect(s.footer).toBeNull();
  });
});
