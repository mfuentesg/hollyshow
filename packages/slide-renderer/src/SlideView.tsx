import type { Slide, Theme } from "@hollyshow/protocol";
import { useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { canvasMeasure, fitFontSize } from "./fit";

export interface SlideViewProps {
  slide: Slide | null;
  theme: Theme;
  /** Hide the text but keep the background ("clear"). */
  hideText?: boolean;
  /** Draws no background so the output can be composited (OBS / lower third). */
  transparent?: boolean;
  /** Pixel size override for thumbnails/tests; otherwise fills the parent. */
  className?: string;
  style?: CSSProperties;
}

function backgroundCss(theme: Theme): string {
  const b = theme.background;
  switch (b.kind) {
    case "color":
      return b.color;
    case "gradient":
      return `linear-gradient(${b.angleDeg}deg, ${b.from}, ${b.to})`;
    default:
      // Image/video layers are rendered by the media layer (Phase 4); solid fallback here.
      return "#000";
  }
}

/**
 * The single slide renderer shared by output, preview, thumbnails, stage and OBS (PLAN §3.1.3).
 * Fills its parent; text is auto-fitted inside the theme's safe area.
 */
export function SlideView({
  slide,
  theme,
  hideText,
  transparent,
  className,
  style,
}: SlideViewProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 1920, h: 1080 });

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(([e]) => {
      if (e) setBox({ w: e.contentRect.width, h: e.contentRect.height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const { font, margins } = theme;
  const lines = useMemo(
    () => (slide ? (slide.bible.length > 0 ? slide.bible.map((c) => c.text) : slide.lines) : []),
    [slide],
  );
  const availW = box.w * (1 - (margins.left + margins.right) / 100);
  const availH = box.h * (1 - (margins.top + margins.bottom) / 100);
  const px = useMemo(
    () =>
      fitFontSize({
        lines,
        width: availW,
        height: availH,
        minPx: font.minPx * (box.h / 1080),
        maxPx: font.maxPx * (box.h / 1080),
        lineHeight: font.lineHeight,
        measure: canvasMeasure(font.family, font.weight),
      }),
    [lines, availW, availH, font, box.h],
  );

  const root: CSSProperties = {
    position: "relative",
    width: "100%",
    height: "100%",
    overflow: "hidden",
    background: transparent ? "transparent" : backgroundCss(theme),
    color: theme.textColor,
    fontFamily: `${font.family}, var(--hs-font-projection, sans-serif)`,
    fontWeight: font.weight,
    fontStyle: font.italic ? "italic" : "normal",
    textTransform: font.uppercase ? "uppercase" : "none",
    textAlign: theme.align,
    lineHeight: font.lineHeight,
    ...style,
  };

  return (
    <div ref={ref} className={className} style={root} data-slide-id={slide?.id ?? ""}>
      {slide && !hideText && (
        <div
          style={{
            position: "absolute",
            inset: `${margins.top}% ${margins.right}% ${margins.bottom}% ${margins.left}%`,
            display: "flex",
            flexDirection: "column",
            justifyContent: "center",
            fontSize: px,
            textShadow: theme.shadow ?? undefined,
          }}
        >
          {slide.bible.length > 0
            ? slide.bible.map((c) => (
                <div key={c.version} style={{ marginBottom: px * 0.4 }}>
                  <div>{c.text}</div>
                  <div style={{ fontSize: px * 0.5, opacity: 0.8 }}>
                    {c.reference}
                    {theme.showBibleVersionLabel ? ` · ${c.version}` : ""}
                  </div>
                </div>
              ))
            : slide.lines.map((l, i) => (
                <div key={i}>
                  {l}
                  {slide.translation[i] && (
                    <div style={{ fontSize: px * theme.translationScale, opacity: 0.85 }}>
                      {slide.translation[i]}
                    </div>
                  )}
                </div>
              ))}
          {slide.footer && theme.showCopyright && (
            <div style={{ fontSize: px * 0.3, opacity: 0.6, marginTop: px * 0.5 }}>
              {slide.footer}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
