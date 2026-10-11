import { describe, expect, it } from "vitest";
import { fitFontSize, fitUniform } from "./fit";

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
