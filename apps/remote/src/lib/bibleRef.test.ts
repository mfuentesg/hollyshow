import { describe, expect, it } from "vitest";
import {
  chapterPassage,
  findBook,
  parseDisplayedReference,
  parseReference,
  stepChapter,
  versePassage,
} from "./bibleRef";

describe("parseReference", () => {
  it("understands the mockup example 'ro 8 28'", () => {
    expect(parseReference("ro 8 28")).toEqual({
      book: "Rom",
      chapter: 8,
      verse: 28,
      verseEnd: null,
    });
  });
  it("accepts colon, dot and ranges", () => {
    expect(parseReference("Romanos 8:28-30")).toEqual({
      book: "Rom",
      chapter: 8,
      verse: 28,
      verseEnd: 30,
    });
    expect(parseReference("jn 3.16")).toEqual({
      book: "John",
      chapter: 3,
      verse: 16,
      verseEnd: null,
    });
    expect(parseReference("jn3:16")).toEqual({
      book: "John",
      chapter: 3,
      verse: 16,
      verseEnd: null,
    });
  });
  it("understands numbered books in both languages", () => {
    expect(parseReference("1 juan 4:8")?.book).toBe("1John");
    expect(parseReference("1 John 4:8")?.book).toBe("1John");
    expect(parseReference("2 Co 5 17")?.book).toBe("2Cor");
  });
  it("opens a whole chapter when there is no verse", () => {
    expect(parseReference("Salmo 23")).toEqual({
      book: "Ps",
      chapter: 23,
      verse: null,
      verseEnd: null,
    });
  });
  it("treats a single number in a one-chapter book as the verse", () => {
    expect(parseReference("judas 5")).toEqual({
      book: "Jude",
      chapter: 1,
      verse: 5,
      verseEnd: null,
    });
  });
  it("ignores accents and dots", () => {
    expect(parseReference("Éx. 20:3")).toEqual({
      book: "Exod",
      chapter: 20,
      verse: 3,
      verseEnd: null,
    });
  });
  it("rejects what is not a reference", () => {
    expect(parseReference("amor perfecto")).toBeNull();
    expect(parseReference("")).toBeNull();
    expect(parseReference("romanos")).toBeNull();
    expect(parseReference("romanos 99")).toBeNull();
    expect(parseReference("xyz 3")).toBeNull();
  });
  it("drops an inverted range", () => {
    expect(parseReference("jn 3:16-10")?.verseEnd).toBeNull();
  });
});

describe("helpers", () => {
  it("findBook prefers exact aliases over prefixes", () => {
    expect(findBook("ro")?.osis).toBe("Rom");
    expect(findBook("gen")?.osis).toBe("Gen");
    expect(findBook("x")).toBeNull();
  });
  it("builds passages", () => {
    expect(chapterPassage("Rom", 8)).toEqual({
      start: { book: "Rom", chapter: 8, verse: null },
      end: { book: "Rom", chapter: 8, verse: null },
    });
    expect(versePassage("Rom", 8, 30, 28)).toEqual({
      start: { book: "Rom", chapter: 8, verse: 28 },
      end: { book: "Rom", chapter: 8, verse: 30 },
    });
  });
  it("steps across chapters and books", () => {
    expect(stepChapter("Rom", 8, 1)).toEqual({ book: "Rom", chapter: 9 });
    expect(stepChapter("Rom", 16, 1)).toEqual({ book: "1Cor", chapter: 1 });
    expect(stepChapter("1Cor", 1, -1)).toEqual({ book: "Rom", chapter: 16 });
    expect(stepChapter("Gen", 1, -1)).toBeNull();
    expect(stepChapter("Rev", 22, 1)).toBeNull();
  });
  it("reads the reference shown on the projector", () => {
    expect(parseDisplayedReference("Juan 3:16–18")).toEqual({
      book: "John",
      chapter: 3,
      verse: 16,
      verseEnd: 18,
    });
    expect(parseDisplayedReference("1 John 3:16")?.book).toBe("1John");
  });
});
