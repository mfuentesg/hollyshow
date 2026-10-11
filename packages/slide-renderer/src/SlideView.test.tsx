import { lyricSlide, type BibleColumn, type Slide } from "@hollyshow/protocol";
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { SlideThumbnail, SlideView, defaultTheme } from "./index";

const bible = (layout: Slide["layout"]): Slide => {
  const cols: BibleColumn[] = [
    {
      version: "RVR1909",
      reference: "Juan 3:16",
      text: "Porque de tal manera",
      copyright: "Dominio público",
    },
    {
      version: "KJV",
      reference: "John 3:16",
      text: "For God so loved",
      copyright: "Public domain",
    },
  ];
  return { ...lyricSlide("b", "Juan 3:16", []), kind: "bible", bible: cols, layout };
};

describe("SlideView", () => {
  it("renders lyric lines", () => {
    const html = renderToString(
      <SlideView slide={lyricSlide("s", "Coro", ["Hola", "Mundo"])} theme={defaultTheme} />,
    );
    expect(html).toContain("Hola");
    expect(html).toContain("Mundo");
  });

  it("keeps the text in the DOM but invisible when cleared (it fades, never unmounts)", () => {
    const html = renderToString(
      <SlideView slide={lyricSlide("s", null, ["Oculto"])} theme={defaultTheme} hideText />,
    );
    expect(html).toMatch(/opacity:0[^"]*"[^>]*><div[^>]*data-slide-frame/);
  });

  it("renders the bilingual second line", () => {
    const s = { ...lyricSlide("s", null, ["Santo"]), translation: ["Holy"] };
    const html = renderToString(<SlideView slide={s} theme={defaultTheme} />);
    expect(html).toContain("Holy");
  });

  it("draws version labels and copyright for parallel Bible slides", () => {
    const html = renderToString(<SlideView slide={bible("columns")} theme={defaultTheme} />);
    expect(html).toContain("RVR1909");
    expect(html).toContain("KJV");
    expect(html).toContain("Dominio público");
    expect(html).toContain("Public domain");
    expect(html).toContain("Juan 3:16");
  });

  it("omits version labels and copyright when the theme says so", () => {
    const theme = { ...defaultTheme, showBibleVersionLabel: false, showCopyright: false };
    const html = renderToString(<SlideView slide={bible("stacked")} theme={theme} />);
    expect(html).not.toContain("RVR1909");
    expect(html).not.toContain("Dominio público");
    expect(html).toContain("Porque de tal manera");
  });

  it("supports the three parallel layouts", () => {
    for (const l of ["columns", "stacked", "subtitle"] as const) {
      const html = renderToString(<SlideView slide={bible(l)} theme={defaultTheme} />);
      expect(html).toContain("For God so loved");
    }
    expect(renderToString(<SlideView slide={bible("columns")} theme={defaultTheme} />)).toContain(
      "grid-template-columns:repeat(2",
    );
  });

  it("transparent mode paints no background", () => {
    const html = renderToString(
      <SlideView slide={lyricSlide("s", null, ["x"])} theme={defaultTheme} transparent />,
    );
    expect(html).toContain("background:transparent");
    expect(html).not.toContain("background:#0F1115");
  });

  it("renders the overlay and the logo layer", () => {
    const html = renderToString(
      <SlideView
        slide={null}
        theme={defaultTheme}
        overlay="Baños al fondo"
        logo={<b>LOGO</b>}
        showLogo
      />,
    );
    expect(html).toContain("Baños al fondo");
    expect(html).toContain("LOGO");
  });

  it("thumbnails do not render the overlay", () => {
    const html = renderToString(
      <SlideThumbnail slide={lyricSlide("s", null, ["Mini"])} theme={defaultTheme} width={192} />,
    );
    expect(html).toContain("Mini");
    expect(html).toContain("scale(0.1)");
  });

  it("applies text outline and shadow", () => {
    const html = renderToString(
      <SlideView
        slide={lyricSlide("s", null, ["x"])}
        theme={defaultTheme}
        outline={{ color: "#000", widthEm: 0.04 }}
      />,
    );
    expect(html).toContain("-webkit-text-stroke");
    expect(html).toContain("text-shadow");
  });

  it("parallel Bible: long credit is one ellipsized line and labels use longhand font styles", () => {
    const slide = bible("columns");
    slide.bible[0]!.copyright = "Reina-Valera 1909 (dominio público) ".repeat(4);
    const html = renderToString(
      <SlideView slide={slide} theme={{ ...defaultTheme, showCopyright: true }} />,
    );
    expect(html).toContain("text-overflow:ellipsis");
    expect(html).toContain("white-space:nowrap");
    expect(html).not.toMatch(/style="[^"]*[; ]font:/);
    expect(html).toContain("RVR1909");
  });
});
