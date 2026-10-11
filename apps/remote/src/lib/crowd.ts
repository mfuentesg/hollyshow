import type { BibleColumn, Slide } from "@hollyshow/protocol";
import { pickColumns } from "./slideText";

export type CrowdLang = "both" | "es" | "en";

export interface CrowdSettings {
  /** Base font size in px for the current slide. */
  size: number;
  dark: boolean;
  lang: CrowdLang;
  /** Bible version to prefer when the projector shows several; null = what the projector shows. */
  version: string | null;
}

export const SIZE_MIN = 16;
export const SIZE_MAX = 44;

export const defaultCrowd = (prefersDark: boolean): CrowdSettings => ({
  size: 28,
  dark: prefersDark,
  lang: "both",
  version: null,
});

export function sanitizeCrowd(raw: unknown, fallback: CrowdSettings): CrowdSettings {
  if (typeof raw !== "object" || raw === null) return fallback;
  const r = raw as Partial<CrowdSettings>;
  return {
    size:
      typeof r.size === "number"
        ? Math.max(SIZE_MIN, Math.min(SIZE_MAX, Math.round(r.size)))
        : fallback.size,
    dark: typeof r.dark === "boolean" ? r.dark : fallback.dark,
    lang: r.lang === "es" || r.lang === "en" || r.lang === "both" ? r.lang : fallback.lang,
    version: typeof r.version === "string" ? r.version : null,
  };
}

export function bumpSize(s: CrowdSettings, delta: number): CrowdSettings {
  return { ...s, size: Math.max(SIZE_MIN, Math.min(SIZE_MAX, s.size + delta)) };
}

export interface CrowdBlock {
  /** Main text (what the projector reads). */
  text: string;
  /** Second language, when the slide has one and the person wants it. */
  translation: string | null;
  /** Bible reference or section label. */
  caption: string | null;
  /** Bible versions available on this slide. */
  versions: string[];
}

/**
 * What a phone shows for a slide. "Español" shows the main lines; "English" prefers the
 * translation when the slide has one; "Español + English" shows both.
 */
export function crowdBlock(
  slide: Slide | null,
  s: Pick<CrowdSettings, "lang" | "version">,
): CrowdBlock | null {
  if (!slide) return null;
  if (slide.kind === "bible" && slide.bible.length > 0) {
    const cols: BibleColumn[] = pickColumns(slide.bible, s.version);
    return {
      text: cols.map((c) => c.text).join("\n\n"),
      translation: null,
      caption: cols.map((c) => `${c.reference} (${c.version})`).join(" · "),
      versions: slide.bible.map((c) => c.version),
    };
  }
  const main = slide.lines.join("\n");
  const trans = slide.translation.join("\n");
  const hasTrans = trans.trim() !== "";
  if (s.lang === "en" && hasTrans) {
    return { text: trans, translation: null, caption: slide.label, versions: [] };
  }
  return {
    text: main,
    translation: s.lang === "both" && hasTrans ? trans : null,
    caption: slide.label,
    versions: [],
  };
}
