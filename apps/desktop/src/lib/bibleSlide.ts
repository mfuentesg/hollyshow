import type { BibleColumn, ParallelLayout, Passage, Slide, Verse } from "@hollyshow/protocol";
import { formatPassage, type BookLang } from "./bibleRef";

export interface VersionText {
  code: string;
  language: string;
  copyright: string | null;
  verses: Verse[];
}

export interface BibleSlideOptions {
  showCopyright: boolean;
  showVersionLabel: boolean;
}

export const defaultBibleOptions: BibleSlideOptions & {
  versePerSlide: boolean;
  wordsOfJesusRed: boolean;
} = {
  showCopyright: true,
  showVersionLabel: true,
  versePerSlide: true,
  wordsOfJesusRed: false,
};

const langOf = (tag: string): BookLang => (tag.toLowerCase().startsWith("en") ? "en" : "es");

/** Joins verse texts with their numbers when there is more than one verse. */
export function passageText(verses: Verse[]): string {
  if (verses.length === 0) return "";
  if (verses.length === 1) return verses[0]!.text;
  return verses.map((v) => `${v.reference.verse ?? ""} ${v.text}`.trim()).join(" ");
}

/**
 * Local composition of a Bible slide, used by the Bible screen preview and the demo backend. The
 * real host composes the slide with `hollyshow_bible::build_bible_slide`.
 */
export function composeBibleSlide(
  id: string,
  passage: Passage,
  texts: VersionText[],
  layout: ParallelLayout,
  opts: BibleSlideOptions,
): Slide {
  const bible: BibleColumn[] = texts.map((t) => ({
    version: t.code,
    reference: formatPassage(passage, langOf(t.language)),
    text: t.verses.length ? passageText(t.verses) : "—",
    copyright: opts.showCopyright ? t.copyright : null,
  }));
  const first = bible[0];
  return {
    id,
    kind: "bible",
    label: first?.reference ?? null,
    lines: [],
    translation: [],
    chart: [],
    bible,
    layout,
    media: null,
    footer: opts.showVersionLabel ? texts.map((t) => t.code).join(" · ") : null,
    themeId: null,
    sizeGroup: null,
  };
}
