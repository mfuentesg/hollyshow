import { describe, expect, it } from "vitest";
import { fitFontSize, fitUniform, textHeight, wrapCount } from "./fit";

const measure = (t: string, px: number) => t.length * px * 0.5;
const base = { width: 1000, height: 400, minPx: 20, maxPx: 200, lineHeight: 1.2, measure };

describe("fitFontSize", () => {
  it("grows short text up to maxPx", () => {
    expect(fitFontSize({ ...base, lines: ["Hi"], maxPx: 150 })).toBe(150);
  });

  it("shrinks long text so it still fits", () => {
    const lines = ["x".repeat(60), "y".repeat(60), "z".repeat(60)];
    const px = fitFontSize({ ...base, lines });
    expect(px).toBeGreaterThanOrEqual(20);
    expect(px).toBeLessThan(200);
    // The next size up must overflow.
    const over = fitFontSize({ ...base, lines, maxPx: px + 1 });
    expect(over).toBe(px);
  });

  it("returns minPx when nothing fits", () => {
    expect(fitFontSize({ ...base, lines: ["x".repeat(5000)], height: 10 })).toBe(20);
  });

  it("uniform size is the minimum over slides", () => {
    const a = { ...base, lines: ["short"] };
    const b = { ...base, lines: ["x".repeat(200)] };
    expect(fitUniform([a, b])).toBe(fitFontSize(b));
  });
});

describe("word wrapping", () => {
  const m = (t: string, px: number) => t.length * px;
  it("counts rows by words, not by raw width", () => {
    // width 100px at 10px/char = 10 chars per row; "aaaa bbbb cccc" -> "aaaa bbbb" (9) + "cccc"
    expect(wrapCount("aaaa bbbb cccc", 100, 10, m)).toBe(2);
    // naive ceil(14/10) would also say 2, but "aaaaaaa bbbbbbb" needs 2 rows (ceil says 2) and
    // "aaaaaa bbbbbb cc" -> rows: "aaaaaa", "bbbbbb cc"(9) = 2
    expect(wrapCount("aaaaaa bbbbbb cc", 100, 10, m)).toBe(2);
    expect(wrapCount("aaaaaaaa bbbbbbbb cccccccc", 100, 10, m)).toBe(3);
  });
  it("hard-breaks a word wider than the box", () => {
    expect(wrapCount("x".repeat(25), 100, 10, m)).toBe(3);
  });
  it("honours explicit newlines and empty lines", () => {
    expect(wrapCount("a\nb", 1000, 10, m)).toBe(2);
    expect(wrapCount("", 1000, 10, m)).toBe(1);
  });
});

describe("scales, gaps and extra height", () => {
  const input = { ...base, width: 100000, height: 300, minPx: 10, maxPx: 400 };
  it("smaller secondary lines allow a bigger base size", () => {
    const plain = fitFontSize({ ...input, lines: ["a", "b"] });
    const scaled = fitFontSize({ ...input, lines: ["a", "b"], scales: [1, 0.5] });
    expect(scaled).toBeGreaterThan(plain);
  });
  it("gap and extra height shrink the size", () => {
    const plain = fitFontSize({ ...input, lines: ["a", "b"] });
    expect(fitFontSize({ ...input, lines: ["a", "b"], gapEm: 1 })).toBeLessThan(plain);
    expect(fitFontSize({ ...input, lines: ["a", "b"], extraEm: 2 })).toBeLessThan(plain);
  });
  it("is monotonic: the answer fits and answer+1 does not (when below max)", () => {
    const i = { ...input, lines: ["hola mundo cruel ".repeat(6)], width: 600 };
    const px = fitFontSize(i);
    expect(textHeight(i, px)).toBeLessThanOrEqual(i.height);
    if (px < i.maxPx) expect(textHeight(i, px + 1)).toBeGreaterThan(i.height);
  });
  it("min equal to max returns that size", () => {
    expect(fitFontSize({ ...input, lines: ["a"], minPx: 50, maxPx: 50 })).toBe(50);
  });
});
