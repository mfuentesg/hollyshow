import type { ParallelLayout, ShowEntry, Slide, Theme } from "@hollyshow/protocol";
import { fitFontSize, type FitInput, type Measure } from "./fit";

export interface Box {
  w: number;
  h: number;
}

/** Reference output height: theme font sizes (minPx/maxPx) are expressed for 1080p. */
export const REFERENCE_HEIGHT = 1080;

/** Relative sizes of the secondary text elements (em of the base size). */
export const SCALE = {
  bibleHeader: 0.8,
  versionLabel: 0.42,
  subtitleSecondary: 0.62,
  footer: 0.3,
} as const;

/** Version labels never drop below ~14px at 1080p equivalent. */
export const MIN_VERSION_LABEL_PX = 14;

export function versionLabelPx(px: number, boxH: number): number {
  return Math.max(px * SCALE.versionLabel, (MIN_VERSION_LABEL_PX * boxH) / REFERENCE_HEIGHT);
}

export type TextMode =
  "lyrics" | "bible-single" | "bible-columns" | "bible-stacked" | "bible-subtitle" | "plain";

export interface TextPlan {
  mode: TextMode;
  /** Base font size in px for the given box. */
  px: number;
  /** Width available to one column (Bible columns) or the whole text area. */
  columnWidth: number;
}

/** Gap between side-by-side Bible columns, as a fraction of the text area width. */
export const COLUMN_GAP = 0.05;

/** Text area in px after applying the theme's safe margins (percent of the output). */
export function textArea(theme: Theme, box: Box, hasFooter: boolean): Box & { bottomPct: number } {
  const m = theme.margins;
  const bottomPct = hasFooter ? Math.max(m.bottom, 7) : m.bottom;
  return {
    w: box.w * (1 - (m.left + m.right) / 100),
    h: box.h * (1 - (m.top + bottomPct) / 100),
    bottomPct,
  };
}

export function modeOf(slide: Slide): TextMode {
  if (slide.bible.length > 1) {
    return slide.layout === "stacked"
      ? "bible-stacked"
      : slide.layout === "subtitle"
        ? "bible-subtitle"
        : "bible-columns";
  }
  if (slide.bible.length === 1) return "bible-single";
  if (slide.kind === "lyrics" || slide.translation.length > 0) return "lyrics";
  return "plain";
}

/** Whether the slide has text that the fit algorithm must lay out. */
export function hasText(slide: Slide): boolean {
  return slide.bible.length > 0 || slide.lines.some((l) => l.length > 0);
}

function fitInput(
  slide: Slide,
  theme: Theme,
  area: Box,
  box: Box,
  measure: Measure,
  mode: TextMode,
): FitInput[] {
  const scale = box.h / REFERENCE_HEIGHT;
  const base = {
    minPx: theme.font.minPx * scale,
    maxPx: theme.font.maxPx * scale,
    lineHeight: theme.font.lineHeight,
    measure,
  };
  const labelPx = 1.2 * ((MIN_VERSION_LABEL_PX * box.h) / REFERENCE_HEIGHT);
  switch (mode) {
    case "bible-columns": {
      const n = slide.bible.length;
      const gap = area.w * COLUMN_GAP;
      const width = (area.w - gap * (n - 1)) / n;
      return slide.bible.map((c) => ({
        ...base,
        lines: [c.text],
        width,
        height: area.h,
        extraEm: SCALE.bibleHeader * theme.font.lineHeight + 0.6 + 0.3,
        extraPx: theme.showBibleVersionLabel ? labelPx : 0,
      }));
    }
    case "bible-stacked": {
      const lines = slide.bible.map((c) => c.text);
      // The first version is the primary one; later ones are drawn smaller (design: 42px / 26px).
      const scales = lines.map((_, i) => (i === 0 ? 1 : 0.62));
      return [
        {
          ...base,
          lines,
          scales,
          width: area.w,
          height: area.h,
          gapEm: 0.7,
          extraEm: SCALE.bibleHeader * theme.font.lineHeight + 0.6,
        },
      ];
    }
    case "bible-subtitle": {
      const lines = slide.bible.map((c) => c.text);
      const scales = lines.map((_, i) => (i === 0 ? 1 : SCALE.subtitleSecondary));
      return [
        {
          ...base,
          lines,
          scales,
          width: area.w,
          height: area.h,
          gapEm: 0.35,
          extraEm: SCALE.bibleHeader * theme.font.lineHeight + 0.4,
        },
      ];
    }
    case "bible-single":
      return [
        {
          ...base,
          lines: slide.bible.map((c) => c.text),
          width: area.w,
          height: area.h,
          extraEm: SCALE.bibleHeader * theme.font.lineHeight + 0.6,
          extraPx: theme.showBibleVersionLabel ? labelPx : 0,
        },
      ];
    case "lyrics":
    case "plain": {
      const lines: string[] = [];
      const scales: number[] = [];
      slide.lines.forEach((l, i) => {
        lines.push(l);
        scales.push(1);
        const tr = slide.translation[i];
        if (tr) {
          lines.push(tr);
          scales.push(theme.translationScale);
        }
      });
      return [{ ...base, lines, scales, width: area.w, height: area.h }];
    }
  }
}

/**
 * Decides layout mode and base font size for a slide in a given output box. Bible columns share
 * one size (the smallest that fits any column) so both translations look balanced.
 */
export function planText(slide: Slide, theme: Theme, box: Box, measure: Measure): TextPlan {
  const mode = modeOf(slide);
  const area = textArea(theme, box, Boolean(theme.showCopyright && (slide.footer || slide.bible.some((c) => c.copyright))),
  );
  const inputs = fitInput(slide, theme, area, box, measure, mode);
  const px =
    inputs.length === 0 ? Math.floor(theme.font.minPx) : Math.min(...inputs.map(fitFontSize));
  const columnWidth =
    mode === "bible-columns"
      ? (area.w - area.w * COLUMN_GAP * (slide.bible.length - 1)) / slide.bible.length
      : area.w;
  return { mode, px, columnWidth };
}

/**
 * Slides that must share one font size with `slide` (song-wide size). Priority: explicit
 * `sizeGroup`, then the whole item when the theme asks for a uniform size.
 */
export function siblingSlides(
  show: ShowEntry[],
  itemId: string | null,
  slide: Slide,
  theme: Theme,
): Slide[] {
  if (!itemId) return [slide];
  const entry = show.find((e) => e.itemId === itemId);
  if (!entry) return [slide];
  if (slide.sizeGroup) return entry.slides.filter((s) => s.sizeGroup === slide.sizeGroup);
  if (theme.font.uniformSize && slide.kind === "lyrics") {
    return entry.slides.filter((s) => s.kind === "lyrics");
  }
  return [slide];
}

/** One font size for a set of slides: the smallest that fits each of them. */
export function groupFontPx(slides: Slide[], theme: Theme, box: Box, measure: Measure): number {
  if (slides.length === 0) return Math.floor(theme.font.minPx);
  return Math.min(...slides.filter(hasText).map((s) => planText(s, theme, box, measure).px));
}

export const layoutLabel: Record<ParallelLayout, string> = {
  columns: "columns",
  stacked: "stacked",
  subtitle: "subtitle",
};
