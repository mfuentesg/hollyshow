import type { BookId, Passage } from "@hollyshow/protocol";
import { BOOKS, type BookInfo } from "./books";

export interface ParsedRef {
  book: BookId;
  chapter: number;
  /** First verse (null = whole chapter). */
  verse: number | null;
  /** Last verse of a range (null = single verse or whole chapter). */
  verseEnd: number | null;
}

/** Lowercase, no accents, no dots: "Éx." -> "ex". */
export function fold(s: string): string {
  return s
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .replace(/[.]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

interface Alias {
  key: string;
  book: BookInfo;
}

const ALIASES: Alias[] = BOOKS.flatMap((book) =>
  [...book.es, ...book.en].map((n) => ({ key: fold(n).replace(/^(\d) /, "$1"), book })),
);

/** Finds a book by exact alias or, failing that, by the first canonical book whose alias starts with `name`. */
export function findBook(name: string): BookInfo | null {
  const key = fold(name).replace(/^(\d) /, "$1");
  if (key.length === 0) return null;
  const exact = ALIASES.find((a) => a.key === key);
  if (exact) return exact.book;
  if (key.length < 2) return null;
  return ALIASES.find((a) => a.key.startsWith(key))?.book ?? null;
}

const REF_RE = /^([1-3]?\s*[a-z][a-z ]*?)\s*(\d.*)$/;
const NUMS_RE = /^(\d+)(?:\s*[:.\s]\s*(\d+)(?:\s*-\s*(\d+))?|\s*-\s*\d+)?$/;

/**
 * Parses typed references: "ro 8 28", "Romanos 8:28-30", "1 jn 3.16", "jn3:16", "Salmo 23".
 * Returns null when the text does not look like a reference (the caller then does a text search).
 */
export function parseReference(input: string): ParsedRef | null {
  const text = fold(input).replace(/[,;]/g, " ");
  const head = REF_RE.exec(text);
  if (!head) return null;
  const book = findBook(head[1] ?? "");
  if (!book) return null;
  const nums = NUMS_RE.exec((head[2] ?? "").trim());
  if (!nums) return null;

  let chapter = Number(nums[1]);
  let verse = nums[2] === undefined ? null : Number(nums[2]);
  let end = nums[3] === undefined ? null : Number(nums[3]);
  if (book.chapters === 1 && verse === null) {
    // "judas 5": a single number in a one-chapter book is a verse.
    verse = chapter;
    chapter = 1;
  } else if (book.chapters === 1 && chapter !== 1) {
    return null;
  }
  if (chapter < 1 || chapter > book.chapters) return null;
  if (verse !== null && verse < 1) return null;
  if (end !== null && (verse === null || end < verse)) end = null;
  return { book: book.osis, chapter, verse, verseEnd: end };
}

export function chapterPassage(book: BookId, chapter: number): Passage {
  return {
    start: { book, chapter, verse: null },
    end: { book, chapter, verse: null },
  };
}

export function versePassage(book: BookId, chapter: number, from: number, to: number): Passage {
  const [a, b] = from <= to ? [from, to] : [to, from];
  return { start: { book, chapter, verse: a }, end: { book, chapter, verse: b } };
}

export function bookInfo(osis: BookId): BookInfo | null {
  return BOOKS.find((b) => b.osis === osis) ?? null;
}

/** Previous / next chapter, crossing book boundaries within the 66-book canon. */
export function stepChapter(
  book: BookId,
  chapter: number,
  delta: 1 | -1,
): { book: BookId; chapter: number } | null {
  const i = BOOKS.findIndex((b) => b.osis === book);
  const info = BOOKS[i];
  if (!info) return null;
  const c = chapter + delta;
  if (c >= 1 && c <= info.chapters) return { book, chapter: c };
  const nb = BOOKS[i + delta];
  if (!nb) return null;
  return { book: nb.osis, chapter: delta === 1 ? 1 : nb.chapters };
}

/**
 * Reads a rendered reference such as "Juan 3:16-18" or "1 John 3:16" back into chapter/verses so
 * the reader can highlight what is on screen. Null when it is not understood.
 */
export function parseDisplayedReference(ref: string): ParsedRef | null {
  return parseReference(ref.replace(/\s*[–—]\s*/g, "-"));
}
