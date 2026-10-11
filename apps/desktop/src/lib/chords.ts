const SHARP = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const FLAT = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];
const IDX: Record<string, number> = {
  C: 0,
  "B#": 0,
  "C#": 1,
  Db: 1,
  D: 2,
  "D#": 3,
  Eb: 3,
  E: 4,
  Fb: 4,
  "E#": 5,
  F: 5,
  "F#": 6,
  Gb: 6,
  G: 7,
  "G#": 8,
  Ab: 8,
  A: 9,
  "A#": 10,
  Bb: 10,
  B: 11,
  Cb: 11,
};

const FLAT_KEYS = new Set([
  "F",
  "Bb",
  "Eb",
  "Ab",
  "Db",
  "Gb",
  "Dm",
  "Gm",
  "Cm",
  "Fm",
  "Bbm",
  "Ebm",
]);

/** Transposes one chord symbol (`F#m7/C#`); unknown text is returned unchanged. */
export function transposeChord(chord: string, semitones: number, preferFlats = false): string {
  const part = (p: string): string => {
    const m = /^([A-G][#b]?)(.*)$/.exec(p);
    if (!m) return p;
    const i = IDX[m[1]!];
    if (i === undefined) return p;
    const n = (((i + semitones) % 12) + 12) % 12;
    return (preferFlats ? FLAT : SHARP)[n]! + m[2]!;
  };
  const [main, bass] = chord.split("/");
  return bass ? `${part(main!)}/${part(bass)}` : part(main!);
}

/** Transposes every `[chord]` of a ChordPro text. */
export function transposeText(text: string, semitones: number, preferFlats = false): string {
  if (semitones % 12 === 0) return text;
  return text.replace(
    /\[([^\]]+)\]/g,
    (_m, c: string) => `[${transposeChord(c, semitones, preferFlats)}]`,
  );
}

export function transposeKey(key: string | null, semitones: number): string | null {
  if (!key) return key;
  const flats = FLAT_KEYS.has(key);
  const out = transposeChord(key, semitones, semitones === 0 ? flats : flats || semitones < 0);
  return out;
}

export function hasChords(text: string): boolean {
  return /\[[A-G][#b]?[^\]]*\]/.test(text);
}
