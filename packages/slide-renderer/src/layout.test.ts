import { lyricSlide } from "@hollyshow/protocol";
import { defaultTheme } from "./index";
import type { BibleColumn, ShowEntry, Slide } from "@hollyshow/protocol";
import { describe, expect, it } from "vitest";
import { groupFontPx, modeOf, planText, siblingSlides, textArea } from "./layout";

const measure = (t: string, px: number) => t.length * px * 0.5;
const box = { w: 1920, h: 1080 };

function bible(layout: Slide["layout"], texts: string[]): Slide {
  const bible: BibleColumn[] = texts.map((text, i) => ({
    version: i === 0 ? "RVR1909" : "KJV",
    reference: "Juan 3:16",
    text,
    copyright: null,
  }));
  return { ...lyricSlide("b", "Juan 3:16", []), kind: "bible", bible, layout };
}

describe("modeOf", () => {
  it("maps slides to layout modes", () => {
    expect(modeOf(lyricSlide("a", null, ["x"]))).toBe("lyrics");
    expect(modeOf(bible("columns", ["a"]))).toBe("bible-single");
    expect(modeOf(bible("columns", ["a", "b"]))).toBe("bible-columns");
    expect(modeOf(bible("stacked", ["a", "b"]))).toBe("bible-stacked");
    expect(modeOf(bible("subtitle", ["a", "b"]))).toBe("bible-subtitle");
  });
});

describe("textArea", () => {
  it("applies safe margins and reserves a footer band", () => {
    const a = textArea(defaultTheme, box, false);
    expect(a.w).toBeCloseTo(1920 * 0.84);
    expect(a.h).toBeCloseTo(1080 * 0.88);
    const withFooter = textArea(defaultTheme, box, true);
    expect(withFooter.h).toBeLessThan(a.h);
  });
});

describe("planText", () => {
  it("fits within min and max scaled to the box height", () => {
    const s = lyricSlide("a", null, ["Hola"]);
    expect(planText(s, defaultTheme, box, measure).px).toBe(defaultTheme.font.maxPx);
    const half = planText(s, defaultTheme, { w: 960, h: 540 }, measure).px;
    expect(half).toBe(defaultTheme.font.maxPx / 2);
  });
  it("long text shrinks, never below min", () => {
    const s = lyricSlide("a", null, ["palabra ".repeat(400)]);
    expect(planText(s, defaultTheme, box, measure).px).toBe(defaultTheme.font.minPx);
  });
  it("columns share the size of the tightest column and use narrower width", () => {
    const short = bible("columns", ["corto", "corto"]);
    const uneven = bible("columns", ["corto", "largo ".repeat(120)]);
    const a = planText(short, defaultTheme, box, measure);
    const b = planText(uneven, defaultTheme, box, measure);
    expect(b.px).toBeLessThan(a.px);
    expect(b.columnWidth).toBeLessThan(textArea(defaultTheme, box, false).w / 2);
  });
  it("translation lines count toward the height", () => {
    const base = lyricSlide("a", null, ["linea ".repeat(8), "otra ".repeat(8)]);
    const bilingual = { ...base, translation: ["trans ".repeat(8), "trans ".repeat(8)] };
    expect(planText(bilingual, defaultTheme, box, measure).px).toBeLessThanOrEqual(
      planText(base, defaultTheme, box, measure).px,
    );
  });
});

describe("size groups", () => {
  const slides = [
    { ...lyricSlide("i:0", "V1", ["corto"]), sizeGroup: "g" },
    { ...lyricSlide("i:1", "V2", ["linea larga ".repeat(40)]), sizeGroup: "g" },
    lyricSlide("i:2", "C", ["aislado"]),
  ];
  const show: ShowEntry[] = [{ itemId: "i", title: "t", slides, durationSecs: null }];

  it("sizeGroup collects the members only", () => {
    const sib = siblingSlides(show, "i", slides[0]!, {
      ...defaultTheme,
      font: { ...defaultTheme.font, uniformSize: false },
    });
    expect(sib.map((s) => s.id)).toEqual(["i:0", "i:1"]);
  });
  it("uniformSize groups the whole lyrics item", () => {
    const sib = siblingSlides(show, "i", slides[2]!, defaultTheme);
    expect(sib).toHaveLength(3);
  });
  it("without item or uniformSize the slide is alone", () => {
    const t = { ...defaultTheme, font: { ...defaultTheme.font, uniformSize: false } };
    expect(siblingSlides(show, null, slides[2]!, t)).toHaveLength(1);
    expect(siblingSlides(show, "i", slides[2]!, t)).toHaveLength(1);
  });
  it("group size is the minimum of the members", () => {
    const g = groupFontPx(slides, defaultTheme, box, measure);
    const own = planText(slides[1]!, defaultTheme, box, measure).px;
    expect(g).toBe(own);
    expect(g).toBeLessThan(planText(slides[0]!, defaultTheme, box, measure).px);
  });
});
