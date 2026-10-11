import { describe, expect, it } from "vitest";
import { formatPassage, parseReference, resolveBook, shortPassage } from "./bibleRef";

describe("bible reference parsing", () => {
  it("parses the mockup input 'jn 3 16-18'", () => {
    const r = parseReference("jn 3 16-18")!;
    expect(r.passage.start).toEqual({ book: "John", chapter: 3, verse: 16 });
    expect(r.passage.end).toEqual({ book: "John", chapter: 3, verse: 18 });
    expect(formatPassage(r.passage)).toBe("Juan 3:16–18");
  });

  it.each([
    ["Juan 3:16", "John", 3, 16, 3, 16],
    ["Jn3.16", "John", 3, 16, 3, 16],
    ["1 co 13", "1Cor", 13, null, 13, null],
    ["I Juan 4:8", "1John", 4, 8, 4, 8],
    ["Sal 23:1-4", "Ps", 23, 1, 23, 4],
    ["Gn 1:1-2:3", "Gen", 1, 1, 2, 3],
    ["Judas 3", "Jude", 1, 3, 1, 3],
    ["john 3:16", "John", 3, 16, 3, 16],
  ])("parses %s", (input, book, c1, v1, c2, v2) => {
    const r = parseReference(input)!;
    expect(r).not.toBeNull();
    expect(r.passage.start).toEqual({ book, chapter: c1, verse: v1 });
    expect(r.passage.end).toEqual({ book, chapter: c2, verse: v2 });
  });

  it("rejects non references and impossible ranges", () => {
    expect(parseReference("santo")).toBeNull();
    expect(parseReference("Juan 99")).toBeNull();
    expect(parseReference("Juan 3:18-16")).toBeNull();
    expect(parseReference("Juan")).toBeNull();
    expect(parseReference("")).toBeNull();
  });

  it("resolves accents, ordinals and unique prefixes", () => {
    expect(resolveBook("Genesis")?.osis).toBe("Gen");
    expect(resolveBook("primera de juan")?.osis).toBe("1John");
    expect(resolveBook("apoc")?.osis).toBe("Rev");
    expect(resolveBook("x")).toBeNull();
  });

  it("formats short labels", () => {
    const r = parseReference("Juan 3:16-18")!;
    expect(shortPassage(r.passage)).toBe("Jn 3:16–18");
    expect(formatPassage(parseReference("Sal 23")!.passage, "en")).toBe("Psalms 23");
  });
});
