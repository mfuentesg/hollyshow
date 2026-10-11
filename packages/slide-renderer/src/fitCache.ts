import type { Slide, Theme } from "@hollyshow/protocol";
import { canvasMeasure } from "./fit";
import { modeOf, planText, type Box, type TextPlan } from "./layout";

const MAX = 400;
const cache = new Map<string, TextPlan>();

/** Bible text is drawn lighter than lyrics (design: 600 vs 800). */
export function textWeight(theme: Theme, slide: Slide): number {
  return slide.bible.length > 0 ? Math.min(theme.font.weight, 600) : theme.font.weight;
}

export function slideSignature(slide: Slide): string {
  return JSON.stringify([
    slide.kind,
    slide.lines,
    slide.translation,
    slide.bible,
    slide.layout,
    slide.footer,
  ]);
}

function themeSignature(theme: Theme, slide: Slide): string {
  const f = theme.font;
  const m = theme.margins;
  return [
    f.family,
    textWeight(theme, slide),
    f.italic,
    f.uppercase,
    f.minPx,
    f.maxPx,
    f.lineHeight,
    m.top,
    m.right,
    m.bottom,
    m.left,
    theme.translationScale,
    theme.showCopyright,
  ].join("|");
}

/** Plan with memoisation: the same slide in the same box is fitted only once (pre-render warms it). */
export function cachedPlan(slide: Slide, theme: Theme, box: Box, epoch = 0): TextPlan {
  const key = `${epoch}#${Math.round(box.w)}x${Math.round(box.h)}#${themeSignature(theme, slide)}#${slideSignature(slide)}`;
  const hit = cache.get(key);
  if (hit) {
    cache.delete(key);
    cache.set(key, hit);
    return hit;
  }
  const f = theme.font;
  const measure = canvasMeasure(f.family, textWeight(theme, slide), f.italic);
  const planSlide = f.uppercase ? upper(slide) : slide;
  const plan = planText(planSlide, theme, box, measure);
  cache.set(key, plan);
  if (cache.size > MAX) {
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
  return plan;
}

function upper(slide: Slide): Slide {
  return {
    ...slide,
    lines: slide.lines.map((l) => l.toUpperCase()),
    translation: slide.translation.map((l) => l.toUpperCase()),
    bible: slide.bible.map((c) => ({ ...c, text: c.text.toUpperCase() })),
  };
}

/** Size shared by `slides` (song-wide). Uses the same cache as single slides. */
export function cachedGroupPx(slides: Slide[], theme: Theme, box: Box, epoch = 0): number {
  const sizes = slides
    .filter((s) => s.bible.length > 0 || s.lines.some((l) => l.length > 0))
    .map((s) => cachedPlan(s, theme, box, epoch).px);
  return sizes.length === 0 ? Math.floor(theme.font.minPx) : Math.min(...sizes);
}

export { modeOf };

/** Test hook. */
export function clearPlanCache(): void {
  cache.clear();
}
