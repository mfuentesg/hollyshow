import { lyricSlide } from "@hollyshow/protocol";
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { SlideView, defaultTheme } from "./index";

describe("SlideView", () => {
  it("renders lyric lines", () => {
    const html = renderToString(
      <SlideView slide={lyricSlide("s", "Coro", ["Hola", "Mundo"])} theme={defaultTheme} />,
    );
    expect(html).toContain("Hola");
    expect(html).toContain("Mundo");
  });

  it("renders no text when cleared", () => {
    const html = renderToString(
      <SlideView slide={lyricSlide("s", null, ["Oculto"])} theme={defaultTheme} hideText />,
    );
    expect(html).not.toContain("Oculto");
  });
});
