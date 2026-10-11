import { lyricSlide } from "@hollyshow/protocol";
import { describe, expect, it } from "vitest";
import { SIZE_MAX, SIZE_MIN, bumpSize, crowdBlock, defaultCrowd, sanitizeCrowd } from "./crowd";

const bilingual = { ...lyricSlide("s", "Verso 1", ["Santo, santo"]), translation: ["Holy, holy"] };

describe("crowdBlock", () => {
  it("shows both languages by default", () => {
    expect(crowdBlock(bilingual, { lang: "both", version: null })).toMatchObject({
      text: "Santo, santo",
      translation: "Holy, holy",
      caption: "Verso 1",
    });
  });
  it("Spanish only hides the translation", () => {
    expect(crowdBlock(bilingual, { lang: "es", version: null })?.translation).toBeNull();
  });
  it("English prefers the translation and falls back to the main text", () => {
    expect(crowdBlock(bilingual, { lang: "en", version: null })?.text).toBe("Holy, holy");
    expect(crowdBlock(lyricSlide("x", null, ["Hola"]), { lang: "en", version: null })?.text).toBe(
      "Hola",
    );
  });
  it("filters Bible columns by the chosen version", () => {
    const slide = {
      ...lyricSlide("b", null, []),
      kind: "bible" as const,
      bible: [
        { version: "RVR1909", reference: "Juan 3:16", text: "Porque", copyright: null },
        { version: "KJV", reference: "John 3:16", text: "For God", copyright: null },
      ],
    };
    const b = crowdBlock(slide, { lang: "both", version: "KJV" });
    expect(b?.text).toBe("For God");
    expect(b?.caption).toBe("John 3:16 (KJV)");
    expect(b?.versions).toEqual(["RVR1909", "KJV"]);
  });
  it("is null without a slide", () => {
    expect(crowdBlock(null, { lang: "both", version: null })).toBeNull();
  });
});

describe("settings", () => {
  it("clamps font size", () => {
    const s = defaultCrowd(true);
    expect(bumpSize({ ...s, size: SIZE_MAX }, 3).size).toBe(SIZE_MAX);
    expect(bumpSize({ ...s, size: SIZE_MIN }, -3).size).toBe(SIZE_MIN);
    expect(bumpSize(s, 3).size).toBe(31);
  });
  it("sanitises stored junk", () => {
    const fb = defaultCrowd(false);
    expect(sanitizeCrowd(null, fb)).toEqual(fb);
    expect(sanitizeCrowd({ size: 999, dark: "x", lang: "fr", version: 7 }, fb)).toEqual({
      ...fb,
      size: SIZE_MAX,
      version: null,
    });
    expect(sanitizeCrowd({ size: 30, dark: true, lang: "en", version: "KJV" }, fb)).toEqual({
      size: 30,
      dark: true,
      lang: "en",
      version: "KJV",
    });
  });
});
