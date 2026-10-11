/**
 * Auto-fit text (PLAN §4): the largest integer font size inside [minPx, maxPx] whose wrapped text
 * fits a box, found with a binary search. Pure and measurement-agnostic so it is unit-testable
 * without a browser; the renderer injects a canvas measurer.
 */

/** Width in px of `text` rendered at `fontPx`. */
export type Measure = (text: string, fontPx: number) => number;

export interface FitInput {
  lines: string[];
  /** Available text box in CSS px. */
  width: number;
  height: number;
  minPx: number;
  maxPx: number;
  lineHeight: number;
  measure: Measure;
  /**
   * Size of each line relative to the base font (same length as `lines`; default 1). Used for
   * translations and Bible references that are drawn smaller than the main text.
   */
  scales?: number[];
  /** Vertical gap between logical lines, in em of the base font (default 0). */
  gapEm?: number;
  /** Fixed extra height (headers, labels, footer) in em of the base font (default 0). */
  extraEm?: number;
}

/**
 * Number of visual lines after greedy word wrapping `text` to `width`. A single word wider than
 * the box is hard-broken (ceil), so very long tokens still count as several lines.
 */
export function wrapCount(text: string, width: number, fontPx: number, measure: Measure): number {
  const w = Math.max(1, width);
  if (text.length === 0) return 1;
  const total = measure(text, fontPx);
  if (total <= w && !text.includes("\n")) return 1;
  let count = 0;
  for (const paragraph of text.split("\n")) {
    const words = paragraph.split(/\s+/).filter(Boolean);
    if (words.length === 0) {
      count += 1;
      continue;
    }
    let current = "";
    let rows = 1;
    for (const word of words) {
      const candidate = current ? `${current} ${word}` : word;
      if (measure(candidate, fontPx) <= w) {
        current = candidate;
        continue;
      }
      if (current) {
        rows += 1;
        current = word;
      } else {
        current = word;
      }
      const wordW = measure(word, fontPx);
      if (wordW > w) {
        // Hard break: the extra rows this word occupies.
        rows += Math.ceil(wordW / w) - 1;
        current = "";
      }
    }
    count += rows;
  }
  return count;
}

/** Number of visual lines after wrapping each logical line to `width`. */
export function wrappedLineCount(
  lines: string[],
  width: number,
  fontPx: number,
  measure: Measure,
): number {
  let total = 0;
  for (const line of lines) total += wrapCount(line, width, fontPx, measure);
  return total;
}

/** Height in px the lines need at base size `px`. */
export function textHeight(input: FitInput, px: number): number {
  const { lines, width, lineHeight, measure, scales, gapEm = 0, extraEm = 0 } = input;
  let h = extraEm * px;
  lines.forEach((line, i) => {
    const s = scales?.[i] ?? 1;
    const size = px * s;
    h += wrapCount(line, width, size, measure) * size * lineHeight;
    if (i > 0) h += gapEm * px;
  });
  return h;
}

/**
 * Largest integer font size in [minPx, maxPx] whose wrapped text fits the box. Returns `minPx`
 * when even that overflows, so text is never invisible (the caller may clip instead).
 */
export function fitFontSize(input: FitInput): number {
  const fits = (px: number) => textHeight(input, px) <= input.height;
  let lo = Math.floor(input.minPx);
  let hi = Math.max(lo, Math.floor(input.maxPx));
  if (!fits(lo)) return lo;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (fits(mid)) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** Shared size for a group of slides (song-wide size): the minimum over its members. */
export function fitUniform(inputs: FitInput[]): number {
  return inputs.length === 0 ? 0 : Math.min(...inputs.map(fitFontSize));
}

let canvas: CanvasRenderingContext2D | null | undefined;

/** Browser measurer using canvas; falls back to a 0.55em average glyph width (SSR/tests). */
export function canvasMeasure(family: string, weight: number, italic = false): Measure {
  return (text, px) => {
    if (canvas === undefined) {
      try {
        canvas =
          typeof document === "undefined"
            ? null
            : document.createElement("canvas").getContext("2d");
      } catch {
        canvas = null;
      }
    }
    if (!canvas) return text.length * px * 0.55;
    canvas.font = `${italic ? "italic " : ""}${weight} ${px}px ${family}`;
    return canvas.measureText(text).width;
  };
}
