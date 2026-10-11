/** Inline ChordPro chart lines -> a chord row aligned above the lyric row (stage display). */

export interface ChordLine {
  /** Monospace chord row, padded so each chord sits over the syllable it belongs to. */
  chords: string;
  lyric: string;
  hasChords: boolean;
}

const CHORD = /\[([^\]]*)\]/g;

/**
 * "[D]Santo, [G]santo" -> chords "D     G", lyric "Santo, santo". A chord is never overlapped by
 * the next one: when two chords are closer than their width the next is pushed one space right.
 */
export function parseChordLine(chart: string): ChordLine {
  let lyric = "";
  let chords = "";
  let last = 0;
  let hasChords = false;
  for (const m of chart.matchAll(CHORD)) {
    const idx = m.index ?? 0;
    lyric += chart.slice(last, idx);
    last = idx + m[0].length;
    const name = m[1] ?? "";
    hasChords = true;
    // Pad the chord row to the lyric position (+1 space if it would touch the previous chord).
    const col = lyric.length;
    if (chords.length > 0 && chords.length >= col) chords += " ";
    while (chords.length < col) chords += " ";
    chords += name;
  }
  lyric += chart.slice(last);
  return { chords, lyric, hasChords };
}

/** Short arrangement-strip key for a section label: "Verso 1" -> "V1", "Coro" -> "C". */
export function sectionKey(label: string | null | undefined): string {
  if (!label) return "·";
  const words = label.trim().split(/\s+/);
  const first = words[0] ?? "";
  const num = words.find((w) => /^\d+$/.test(w));
  const letter = first.charAt(0).toUpperCase();
  return `${letter}${num ?? ""}` || "·";
}
