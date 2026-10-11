import type { BibleRef, Passage } from "@hollyshow/protocol";
import { BOOKS, type BookInfo } from "./books";
import { fold } from "./text";

export type BookLang = "es" | "en";

const ORDINALS: Record<string, string> = {
  "1": "1",
  i: "1",
  "1o": "1",
  "1a": "1",
  "1st": "1",
  "1ra": "1",
  "1ro": "1",
  "1er": "1",
  primero: "1",
  primera: "1",
  first: "1",
  "2": "2",
  ii: "2",
  "2o": "2",
  "2a": "2",
  "2nd": "2",
  "2da": "2",
  "2do": "2",
  segundo: "2",
  segunda: "2",
  second: "2",
  "3": "3",
  iii: "3",
  "3o": "3",
  "3a": "3",
  "3rd": "3",
  "3ra": "3",
  "3ro": "3",
  "3er": "3",
  tercero: "3",
  tercera: "3",
  third: "3",
  "4": "4",
  iv: "4",
  "4o": "4",
  "4a": "4",
  "4th": "4",
  "4ta": "4",
  "4to": "4",
  cuarto: "4",
  cuarta: "4",
  fourth: "4",
};

/** Lookup key: folded, ordinal prefix turned into a digit, non-alphanumerics removed. */
export function bookKey(input: string): string {
  const words = fold(input.trim())
    .replace(/[ºª]/g, "")
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .split(/\s+/)
    .filter(Boolean);
  if (words.length === 0) return "";
  let prefix = "";
  const first = words[0]!;
  if (words.length > 1 && ORDINALS[first]) {
    prefix = ORDINALS[first]!;
    words.shift();
    if (words[0] === "de" || words[0] === "del" || words[0] === "of") words.shift();
  }
  return prefix + words.join("");
}

const aliasCache = new Map<BookLang | "any", Map<string, BookInfo>>();

function aliases(lang: BookLang | "any"): Map<string, BookInfo> {
  let m = aliasCache.get(lang);
  if (m) return m;
  m = new Map();
  const langs: BookLang[] = lang === "any" ? ["es", "en"] : [lang, lang === "es" ? "en" : "es"];
  for (const l of langs) {
    for (const b of BOOKS) {
      for (const n of b[l]) if (!m.has(bookKey(n))) m.set(bookKey(n), b);
    }
  }
  for (const b of BOOKS) if (!m.has(bookKey(b.osis))) m.set(bookKey(b.osis), b);
  aliasCache.set(lang, m);
  return m;
}

/** Resolves "Juan", "jn", "1 Cor", "I Juan"; a unique prefix of >= 3 letters also works ("apoc"). */
export function resolveBook(name: string, lang: BookLang = "es"): BookInfo | null {
  const key = bookKey(name);
  if (!key) return null;
  const map = aliases(lang);
  const exact = map.get(key);
  if (exact) return exact;
  if (key.replace(/^\d/, "").length < 3) return null;
  const hits = new Set<BookInfo>();
  for (const [k, b] of map) if (k.startsWith(key)) hits.add(b);
  return hits.size === 1 ? [...hits][0]! : null;
}

export function bookByOsis(osis: string): BookInfo | undefined {
  return BOOKS.find((b) => b.osis === osis);
}

export function bookName(osis: string, lang: BookLang = "es"): string {
  return bookByOsis(osis)?.[lang][0] ?? osis;
}

export interface ParsedReference {
  passage: Passage;
  book: BookInfo;
}

const ref = (book: string, chapter: number, verse: number | null): BibleRef => ({
  book,
  chapter,
  verse,
});

/**
 * Parses a single contiguous reference. Accepts "jn 3 16-18", "Juan 3:16-18", "Jn3.16",
 * "1 co 13", "Sal 23", "Gn 1:1-2:3" and single-chapter books ("Judas 3"). Returns null when the
 * text is not a reference (the palette then treats it as a search).
 */
export function parseReference(input: string, lang: BookLang = "es"): ParsedReference | null {
  const m = /^\s*((?:[1-4]|i{1,3}|iv)?\s*[\p{L}][\p{L}\s.]*?)\s*(\d[\d\s:.,\-–—]*)?\s*$/iu.exec(
    input,
  );
  if (!m) return null;
  const book = resolveBook(m[1]!.replace(/\.+$/, ""), lang);
  if (!book) return null;
  const rest = (m[2] ?? "").trim();
  const single = book.chapters === 1;

  if (!rest) {
    if (!single) return null;
    return { book, passage: { start: ref(book.osis, 1, null), end: ref(book.osis, 1, null) } };
  }
  const nums = rest.replace(/[–—]/g, "-");
  // chapter:verse-chapter:verse
  let r = /^(\d+)\s*[:.\s]\s*(\d+)\s*-\s*(\d+)\s*[:.]\s*(\d+)$/.exec(nums);
  if (r && !single)
    return finish(book, ref(book.osis, +r[1]!, +r[2]!), ref(book.osis, +r[3]!, +r[4]!));
  // chapter:verse-verse
  r = /^(\d+)\s*[:.\s]\s*(\d+)\s*-\s*(\d+)$/.exec(nums);
  if (r && !single)
    return finish(book, ref(book.osis, +r[1]!, +r[2]!), ref(book.osis, +r[1]!, +r[3]!));
  // chapter:verse
  r = /^(\d+)\s*[:.\s]\s*(\d+)$/.exec(nums);
  if (r && !single)
    return finish(book, ref(book.osis, +r[1]!, +r[2]!), ref(book.osis, +r[1]!, +r[2]!));
  // chapter-chapter
  r = /^(\d+)\s*-\s*(\d+)$/.exec(nums);
  if (r && !single) return finish(book, ref(book.osis, +r[1]!, null), ref(book.osis, +r[2]!, null));
  // single-chapter book: verse(-verse)
  if (single) {
    r = /^(\d+)(?:\s*-\s*(\d+))?$/.exec(nums);
    if (r) return finish(book, ref(book.osis, 1, +r[1]!), ref(book.osis, 1, +(r[2] ?? r[1]!)));
  }
  // whole chapter
  r = /^(\d+)$/.exec(nums);
  if (r && !single) return finish(book, ref(book.osis, +r[1]!, null), ref(book.osis, +r[1]!, null));
  return null;
}

function finish(book: BookInfo, start: BibleRef, end: BibleRef): ParsedReference | null {
  const bad = (n: number | null) => n !== null && n < 1;
  if (bad(start.verse) || bad(end.verse) || start.chapter < 1 || end.chapter < 1) return null;
  if (start.chapter > book.chapters || end.chapter > book.chapters) return null;
  const k = (r: BibleRef) => r.chapter * 1000 + (r.verse ?? 0);
  if (k(end) < k(start)) return null;
  return { book, passage: { start, end } };
}

/** `Juan 3:16–18`, `Génesis 1:1–2:3`, `Salmos 23`, `Judas 3`. */
export function formatPassage(p: Passage, lang: BookLang = "es"): string {
  const book = bookByOsis(p.start.book);
  const name = bookName(p.start.book, lang);
  const single = book?.chapters === 1;
  const { start: s, end: e } = p;
  if (s.book !== e.book)
    return `${formatPassage({ start: s, end: s }, lang)} – ${formatPassage({ start: e, end: e }, lang)}`;
  if (s.verse === null && e.verse === null) {
    if (single) return name;
    return s.chapter === e.chapter ? `${name} ${s.chapter}` : `${name} ${s.chapter}–${e.chapter}`;
  }
  const sv = s.verse ?? 1;
  if (s.chapter === e.chapter) {
    const ev = e.verse ?? sv;
    const body = single ? `${sv}` : `${s.chapter}:${sv}`;
    return ev === sv ? `${name} ${body}` : `${name} ${body}–${ev}`;
  }
  return `${name} ${s.chapter}:${sv}–${e.chapter}:${e.verse ?? ""}`;
}

/** Short mono label for chips and history: `Jn 3:16-18`. */
export function shortPassage(p: Passage, lang: BookLang = "es"): string {
  const abbr = bookByOsis(p.start.book)?.[lang][1];
  const full = formatPassage(p, lang);
  const name = bookName(p.start.book, lang);
  return abbr && full.startsWith(name) ? abbr + full.slice(name.length) : full;
}

export function samePassage(a: Passage, b: Passage): boolean {
  const eq = (x: BibleRef, y: BibleRef) =>
    x.book === y.book && x.chapter === y.chapter && x.verse === y.verse;
  return eq(a.start, b.start) && eq(a.end, b.end);
}
