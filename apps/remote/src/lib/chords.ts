/** Chord display helpers for the musician view: ChordPro parsing and transposition. */

export interface ChartSegment {
  /** Chord printed above `text`, if any. */
  chord: string | null;
  text: string;
}

const CHORD_RE = /\[([^\]]*)\]/g;

/** "[D]Santo, [G]santo" -> [{D,"Santo, "},{G,"santo"}]. Text before the first chord has `chord: null`. */
export function parseChart(line: string): ChartSegment[] {
  const out: ChartSegment[] = [];
  let last = 0;
  let pending: string | null = null;
  for (const m of line.matchAll(CHORD_RE)) {
    const idx = m.index ?? 0;
    const text = line.slice(last, idx);
    if (text !== "" || pending !== null) out.push({ chord: pending, text });
    pending = m[1] ?? "";
    last = idx + m[0].length;
  }
  const tail = line.slice(last);
  if (tail !== "" || pending !== null) out.push({ chord: pending, text: tail });
  return out;
}

export function hasChords(line: string): boolean {
  return /\[[^\]]*\]/.test(line);
}

const SHARP = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const FLAT = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];
const LATIN = ["Do", "Re", "Mi", "Fa", "Sol", "La", "Si"];
const LATIN_SEMITONE = [0, 2, 4, 5, 7, 9, 11];
const LETTER_SEMITONE: Record<string, number> = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 };

interface Root {
  semitone: number;
  flat: boolean;
  latin: boolean;
  rest: string;
}

function parseRoot(s: string): Root | null {
  const latin = /^(Do|Re|Mi|Fa|Sol|La|Si)(#|b)?/i.exec(s);
  if (latin) {
    const name = (latin[1] ?? "").toLowerCase();
    const i = LATIN.findIndex((n) => n.toLowerCase() === name);
    const acc = latin[2];
    return {
      semitone: (LATIN_SEMITONE[i] ?? 0) + (acc === "#" ? 1 : acc === "b" ? -1 : 0),
      flat: acc === "b",
      latin: true,
      rest: s.slice(latin[0].length),
    };
  }
  const m = /^([A-G])(#|b)?/.exec(s);
  if (!m) return null;
  const acc = m[2];
  return {
    semitone: (LETTER_SEMITONE[m[1] ?? "C"] ?? 0) + (acc === "#" ? 1 : acc === "b" ? -1 : 0),
    flat: acc === "b",
    latin: false,
    rest: s.slice(m[0].length),
  };
}

const mod12 = (n: number) => ((n % 12) + 12) % 12;

function spell(semitone: number, flat: boolean, latin: boolean): string {
  const name = (flat ? FLAT : SHARP)[mod12(semitone)] ?? "C";
  if (!latin) return name;
  const letter = LETTER_SEMITONE[name.charAt(0)] ?? 0;
  const li = LATIN_SEMITONE.indexOf(letter);
  return `${LATIN[li] ?? "Do"}${name.slice(1)}`;
}

/**
 * Transposes one chord symbol ("F#m7", "Bb/D", "Sol", "Am(add9)") by `semitones`.
 * Keeps the quality/extension text and the accidental style of the original (flats stay flats).
 * Text that is not a chord (e.g. "N.C.", "%") is returned unchanged.
 */
export function transposeChord(chord: string, semitones: number): string {
  const trimmed = chord.trim();
  if (trimmed === "" || mod12(semitones) === 0) return chord;
  const root = parseRoot(trimmed);
  if (!root) return chord;
  const newRoot = spell(root.semitone + semitones, root.flat, root.latin);
  let rest = root.rest;
  const slash = rest.lastIndexOf("/");
  if (slash >= 0) {
    const bass = parseRoot(rest.slice(slash + 1));
    if (bass && bass.rest === "") {
      rest = `${rest.slice(0, slash)}/${spell(bass.semitone + semitones, bass.flat || root.flat, bass.latin)}`;
    }
  }
  return newRoot + rest;
}

/** Transposes every `[chord]` of a ChordPro line. */
export function transposeChart(line: string, semitones: number): string {
  if (mod12(semitones) === 0) return line;
  return line.replace(CHORD_RE, (_m, c: string) => `[${transposeChord(c, semitones)}]`);
}

/** "+2", "-1", "0" label for the transpose control. */
export function semitoneLabel(n: number): string {
  return n > 0 ? `+${n}` : String(n);
}
