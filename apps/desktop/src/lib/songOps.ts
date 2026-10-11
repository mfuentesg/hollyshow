import type { Arrangement, Section, SectionKind, Slide, Song } from "@hollyshow/protocol";
import { transposeText } from "./chords";
import { newSection, defaultLabel, splitLyrics, stripChords } from "./songText";
import { uid } from "./text";

/** Pure song edits used by the editor (each returns a new Song, history keeps the old ones). */

export function patchSong(s: Song, patch: Partial<Song>): Song {
  return { ...s, ...patch };
}

export function updateSection(s: Song, id: string, patch: Partial<Section>): Song {
  return {
    ...s,
    sections: s.sections.map((x) => (x.id === id ? { ...x, ...patch, id: x.id } : x)),
  };
}

export function addSection(s: Song, kind: SectionKind): { song: Song; section: Section } {
  const section = newSection(kind, s.sections, defaultLabel(kind, s.sections));
  const arrangements = s.arrangements.length
    ? s.arrangements.map((a, i) => (i === 0 ? { ...a, sequence: [...a.sequence, section.id] } : a))
    : [{ id: uid("arr"), name: "Original", sequence: [section.id] }];
  return { song: { ...s, sections: [...s.sections, section], arrangements }, section };
}

export function removeSection(s: Song, id: string): Song {
  return {
    ...s,
    sections: s.sections.filter((x) => x.id !== id),
    arrangements: s.arrangements.map((a) => ({
      ...a,
      sequence: a.sequence.filter((x) => x !== id),
    })),
  };
}

export function moveSection(s: Song, from: number, to: number): Song {
  if (from === to || from < 0 || from >= s.sections.length) return s;
  const target = Math.min(Math.max(to, 0), s.sections.length - 1);
  const sections = [...s.sections];
  const [x] = sections.splice(from, 1);
  sections.splice(target, 0, x!);
  return { ...s, sections };
}

export function duplicateSection(s: Song, id: string): Song {
  const i = s.sections.findIndex((x) => x.id === id);
  if (i < 0) return s;
  const copy = { ...s.sections[i]!, id: uid("sec") };
  const sections = [...s.sections];
  sections.splice(i + 1, 0, copy);
  return { ...s, sections };
}

export function addArrangement(s: Song, name: string): { song: Song; arrangement: Arrangement } {
  const arrangement: Arrangement = { id: uid("arr"), name, sequence: s.sections.map((x) => x.id) };
  return { song: { ...s, arrangements: [...s.arrangements, arrangement] }, arrangement };
}

export function updateArrangement(s: Song, id: string, patch: Partial<Arrangement>): Song {
  return {
    ...s,
    arrangements: s.arrangements.map((a) => (a.id === id ? { ...a, ...patch, id: a.id } : a)),
  };
}

export function removeArrangement(s: Song, id: string): Song {
  return { ...s, arrangements: s.arrangements.filter((a) => a.id !== id) };
}

/** Moves one chip of an arrangement sequence. */
export function moveInSequence(seq: string[], from: number, to: number): string[] {
  if (from === to || from < 0 || from >= seq.length) return seq;
  const target = Math.min(Math.max(to, 0), seq.length - 1);
  const out = [...seq];
  const [x] = out.splice(from, 1);
  out.splice(target, 0, x!);
  return out;
}

export interface SongSlideOptions {
  arrangementId: string | null;
  maxLines: number;
  transpose?: number;
  translation?: boolean;
  /** Where the copyright line goes. */
  copyright?: "first" | "last" | "none";
  /** Share one font size across the song. */
  uniformSize?: boolean;
  themeId?: string | null;
  idPrefix?: string;
}

/** Expands a song into slides in arrangement order (the host does the same in `hollyshow_songs`). */
export function songSlides(song: Song, o: SongSlideOptions): Slide[] {
  const arr = song.arrangements.find((a) => a.id === o.arrangementId) ?? song.arrangements[0];
  const order = arr ? arr.sequence : song.sections.map((x) => x.id);
  const out: Slide[] = [];
  const prefix = o.idPrefix ?? "song";
  for (const sid of order) {
    const sec = song.sections.find((x) => x.id === sid);
    if (!sec) continue;
    const lyrics = transposeText(sec.lyrics, o.transpose ?? 0);
    const translation = o.translation && sec.translation ? sec.translation.split("\n") : [];
    splitLyrics(lyrics, o.maxLines).forEach((block, bi) => {
      out.push({
        id: `${prefix}:${out.length}`,
        kind: "lyrics",
        label: sec.label,
        lines: block.map(stripChords),
        translation: bi === 0 ? translation.slice(0, block.length) : [],
        chart: block,
        bible: [],
        layout: "columns",
        media: null,
        footer: null,
        themeId: o.themeId ?? null,
        sizeGroup: o.uniformSize === false ? null : song.id,
      });
    });
  }
  const text = song.copyright ?? null;
  if (text && o.copyright !== "none" && out.length > 0) {
    const idx = o.copyright === "last" ? out.length - 1 : 0;
    out[idx] = { ...out[idx]!, footer: text };
  }
  return out;
}

export function songIsEmpty(s: Song): boolean {
  return s.title.trim() === "" && s.sections.every((x) => x.lyrics.trim() === "");
}
