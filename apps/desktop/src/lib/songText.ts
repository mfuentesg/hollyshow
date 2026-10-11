import type { Arrangement, Section, SectionKind, Song } from "@hollyshow/protocol";
import { uid } from "./text";

/** Splits lyrics into slide texts: `---` forces a break, blank lines separate, then max-lines. */
export function splitLyrics(lyrics: string, maxLines: number): string[][] {
  const max = Math.max(1, maxLines);
  const blocks: string[][] = [];
  let cur: string[] = [];
  const flush = () => {
    if (cur.length) blocks.push(cur);
    cur = [];
  };
  for (const raw of lyrics.replace(/\r\n?/g, "\n").split("\n")) {
    const line = raw.trimEnd();
    if (/^\s*-{3,}\s*$/.test(line) || line.trim() === "") flush();
    else cur.push(line);
  }
  flush();
  // Balanced chunks, e.g. 5 lines with max 4 -> 3 + 2 rather than 4 + 1.
  return blocks.flatMap((b) => {
    if (b.length <= max) return [b];
    const parts = Math.ceil(b.length / max);
    const base = Math.floor(b.length / parts);
    const extra = b.length % parts;
    const out: string[][] = [];
    let at = 0;
    for (let i = 0; i < parts; i++) {
      const n = base + (i < extra ? 1 : 0);
      out.push(b.slice(at, at + n));
      at += n;
    }
    return out;
  });
}

export function countSlides(lyrics: string, maxLines: number): number {
  return splitLyrics(lyrics, maxLines).length;
}

/** Removes ChordPro chords `[G]` for display on slides. */
export function stripChords(line: string): string {
  return line.replace(/\[[^\]]*\]/g, "");
}

const KIND_PREFIX: Record<SectionKind, string> = {
  verse: "V",
  chorus: "C",
  preChorus: "P",
  bridge: "U",
  intro: "I",
  outro: "F",
  tag: "T",
  interlude: "N",
  other: "S",
};

export const SECTION_KINDS: SectionKind[] = [
  "verse",
  "chorus",
  "preChorus",
  "bridge",
  "intro",
  "outro",
  "tag",
  "interlude",
  "other",
];

/** Short code of every section for arrangement chips: V1 V2 C P U F... Numbered per kind. */
export function sectionCodes(sections: Section[]): Map<string, string> {
  const count = new Map<SectionKind, number>();
  const total = new Map<SectionKind, number>();
  for (const s of sections) total.set(s.kind, (total.get(s.kind) ?? 0) + 1);
  const out = new Map<string, string>();
  for (const s of sections) {
    const n = (count.get(s.kind) ?? 0) + 1;
    count.set(s.kind, n);
    const needsNumber = s.kind === "verse" || (total.get(s.kind) ?? 0) > 1;
    out.set(s.id, KIND_PREFIX[s.kind] + (needsNumber ? n : ""));
  }
  return out;
}

export function defaultArrangement(sections: Section[], name = "Original"): Arrangement {
  return { id: uid("arr"), name, sequence: sections.map((s) => s.id) };
}

export function newSection(kind: SectionKind, existing: Section[], label: string): Section {
  return { id: uid("sec"), kind, label, lyrics: "", translation: null };
}

export function emptySong(title = ""): Song {
  const now = Date.now();
  const verse: Section = {
    id: uid("sec"),
    kind: "verse",
    label: "Verso 1",
    lyrics: "",
    translation: null,
  };
  return {
    id: uid("song"),
    title,
    altTitle: null,
    author: null,
    copyright: null,
    ccli: null,
    key: null,
    bpm: null,
    tags: [],
    themeId: null,
    sections: [verse],
    arrangements: [{ id: uid("arr"), name: "Original", sequence: [verse.id] }],
    createdAt: now,
    updatedAt: now,
  };
}

const LABEL_RE =
  /^\s*[[(]?\s*(verso|verse|estrofa|coro|chorus|estribillo|pre[\s-]?coro|pre[\s-]?chorus|puente|bridge|intro|final|outro|tag|interludio|interlude)\s*(\d+)?\s*[\])]?\s*:?\s*$/i;

function kindOfLabel(word: string): SectionKind {
  const w = word.toLowerCase().replace(/[\s-]/g, "");
  if (/^(verso|verse|estrofa)$/.test(w)) return "verse";
  if (/^(coro|chorus|estribillo)$/.test(w)) return "chorus";
  if (/^prec/.test(w)) return "preChorus";
  if (/^(puente|bridge)$/.test(w)) return "bridge";
  if (w === "intro") return "intro";
  if (/^(final|outro)$/.test(w)) return "outro";
  if (w === "tag") return "tag";
  return "interlude";
}

const KIND_LABEL: Record<SectionKind, string> = {
  verse: "Verso",
  chorus: "Coro",
  preChorus: "Pre-coro",
  bridge: "Puente",
  intro: "Intro",
  outro: "Final",
  tag: "Tag",
  interlude: "Interludio",
  other: "Sección",
};

export function defaultLabel(kind: SectionKind, existing: Section[]): string {
  const same = existing.filter((s) => s.kind === kind).length;
  return kind === "verse" || same > 0 ? `${KIND_LABEL[kind]} ${same + 1}` : KIND_LABEL[kind];
}

/**
 * Proposes sections for a pasted full lyric (SongEditor: "pega una letra completa y HollyShow
 * propone las secciones"). Explicit headings ("Coro:", "[Verso 2]") win; otherwise blocks that
 * repeat verbatim become a chorus and the rest are numbered verses. Returns sections plus an
 * arrangement that replays repeated blocks.
 */
export function suggestSections(text: string): { sections: Section[]; sequence: string[] } {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  interface Block {
    heading: string | null;
    kind: SectionKind | null;
    body: string[];
  }
  const blocks: Block[] = [];
  const open = (b: Partial<Block> = {}): Block => {
    const nb: Block = { heading: null, kind: null, body: [], ...b };
    blocks.push(nb);
    return nb;
  };
  let blank = true;
  for (const raw of lines) {
    const line = raw.trimEnd();
    const h = LABEL_RE.exec(line);
    if (h) {
      open({ heading: line.trim(), kind: kindOfLabel(h[1]!) });
      blank = false;
      continue;
    }
    if (line.trim() === "") {
      blank = true;
      continue;
    }
    let cur = blocks[blocks.length - 1];
    if (!cur || (blank && cur.body.length > 0)) cur = open();
    cur.body.push(line);
    blank = false;
  }

  const sections: Section[] = [];
  const sequence: string[] = [];
  const seen = new Map<string, Section>();
  const norm = (b: string[]) =>
    b
      .join("\n")
      .toLowerCase()
      .replace(/[^\p{L}\p{N}\n]+/gu, " ")
      .trim();
  const counts = new Map<string, number>();
  for (const b of blocks)
    if (b.body.length) counts.set(norm(b.body), (counts.get(norm(b.body)) ?? 0) + 1);

  for (const b of blocks) {
    if (b.body.length === 0) continue;
    const key = norm(b.body);
    const prev = seen.get(key);
    if (prev) {
      sequence.push(prev.id);
      continue;
    }
    const repeated = (counts.get(key) ?? 0) > 1;
    const kind: SectionKind = b.kind ?? (repeated ? "chorus" : "verse");
    const label = b.heading
      ? b.heading
          .replace(/[[\]():]/g, "")
          .trim()
          .replace(/^./, (c) => c.toUpperCase())
      : defaultLabel(kind, sections);
    const sec: Section = {
      id: uid("sec"),
      kind,
      label,
      lyrics: b.body.join("\n"),
      translation: null,
    };
    sections.push(sec);
    seen.set(key, sec);
    sequence.push(sec.id);
  }
  return { sections, sequence };
}

export function songToPlainText(song: Song): string {
  return song.sections.map((s) => `${s.label}\n${s.lyrics}`).join("\n\n");
}
