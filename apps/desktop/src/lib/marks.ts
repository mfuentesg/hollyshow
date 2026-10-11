import type { TextMark } from "@hollyshow/protocol";

export interface Span {
  text: string;
  jesus: boolean;
  italic: boolean;
}

/** Splits verse text into styled spans from `TextMark`s (offsets are Unicode scalars, not UTF-16). */
export function applyMarks(text: string, marks: TextMark[]): Span[] {
  const chars = Array.from(text);
  if (marks.length === 0) return [{ text, jesus: false, italic: false }];
  const cuts = new Set<number>([0, chars.length]);
  for (const m of marks) {
    cuts.add(Math.max(0, Math.min(chars.length, m.start)));
    cuts.add(Math.max(0, Math.min(chars.length, m.end)));
  }
  const pts = [...cuts].sort((a, b) => a - b);
  const out: Span[] = [];
  for (let i = 0; i < pts.length - 1; i++) {
    const [a, b] = [pts[i]!, pts[i + 1]!];
    if (a === b) continue;
    const inMark = (k: TextMark["kind"]) =>
      marks.some((m) => m.kind === k && m.start <= a && m.end >= b);
    out.push({
      text: chars.slice(a, b).join(""),
      jesus: inMark("wordsOfJesus"),
      italic: inMark("italic"),
    });
  }
  return out;
}
