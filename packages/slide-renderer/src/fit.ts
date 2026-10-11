export interface FitInput {
  lines: string[];
  /** Available text box in CSS px. */
  width: number;
  height: number;
  minPx: number;
  maxPx: number;
  lineHeight: number;
  /** Width in px of `text` rendered at `fontPx`. Injected so the algorithm is testable. */
  measure: (text: string, fontPx: number) => number;
}

/** Number of visual lines after wrapping each logical line to `width`. */
export function wrappedLineCount(
  lines: string[],
  width: number,
  fontPx: number,
  measure: FitInput["measure"],
): number {
  let total = 0;
  for (const line of lines) {
    const w = measure(line, fontPx);
    total += Math.max(1, Math.ceil(w / Math.max(1, width)));
  }
  return total;
}

/**
 * Largest integer font size in [minPx, maxPx] whose wrapped text fits the box (PLAN §4:
 * binary search). Returns `minPx` when even that overflows, so text is never invisible.
 */
export function fitFontSize(input: FitInput): number {
  const { lines, width, height, minPx, maxPx, lineHeight, measure } = input;
  const fits = (px: number) =>
    wrappedLineCount(lines, width, px, measure) * px * lineHeight <= height;
  let lo = Math.floor(minPx);
  let hi = Math.floor(maxPx);
  if (!fits(lo)) return lo;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (fits(mid)) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** Shared size for a whole item (theme.font.uniformSize): the minimum over its slides. */
export function fitUniform(inputs: FitInput[]): number {
  return inputs.length === 0 ? 0 : Math.min(...inputs.map(fitFontSize));
}

let canvas: CanvasRenderingContext2D | null | undefined;

/** Browser measurer using canvas; falls back to a 0.55em average glyph width (SSR/tests). */
export function canvasMeasure(family: string, weight: number): FitInput["measure"] {
  return (text, px) => {
    if (canvas === undefined) {
      canvas =
        typeof document === "undefined" ? null : document.createElement("canvas").getContext("2d");
    }
    if (!canvas) return text.length * px * 0.55;
    canvas.font = `${weight} ${px}px ${family}`;
    return canvas.measureText(text).width;
  };
}
