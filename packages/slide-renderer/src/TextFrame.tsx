import type { BibleColumn, Slide, Theme } from "@hollyshow/protocol";
import { forwardRef, type CSSProperties, type ReactNode } from "react";
import { textWeight } from "./fitCache";
import { COLUMN_GAP, SCALE, textArea, type Box, type TextMode } from "./layout";

export const ACCENT = "#F2B544";
const MONO = "var(--hs-font-mono, 'JetBrains Mono', ui-monospace, monospace)";

export interface Outline {
  color: string;
  /** Stroke width in em of the base font size. */
  widthEm: number;
}

export interface TextFrameData {
  id: string;
  slide: Slide;
  theme: Theme;
  mode: TextMode;
  /** Base font size in px (already fitted / group-shared). */
  px: number;
  /** Right-hand footer text, e.g. "V1 · 2/3". */
  position?: string;
  outline?: Outline | null;
}

const layerBase: CSSProperties = {
  position: "absolute",
  inset: 0,
  willChange: "opacity, transform",
};

function Line({ children, style }: { children: ReactNode; style?: CSSProperties }) {
  return (
    <div data-line="" style={{ overflowWrap: "anywhere", ...style }}>
      {children}
    </div>
  );
}

function versionLine(c: BibleColumn, theme: Theme, px: number): ReactNode {
  if (!theme.showBibleVersionLabel) return null;
  return (
    <Line
      style={{
        font: `600 ${px * SCALE.versionLabel}px ${MONO}`,
        color: ACCENT,
        textShadow: "none",
        lineHeight: 1.2,
      }}
    >
      {c.version}
    </Line>
  );
}

function BibleHeader({ text, px }: { text: string; px: number }) {
  return (
    <Line
      style={{
        display: "flex",
        alignItems: "center",
        gap: px * 0.4,
        fontWeight: 800,
        fontSize: px * SCALE.bibleHeader,
        lineHeight: 1.2,
        marginBottom: px * 0.6,
        justifyContent: "inherit",
      }}
    >
      <span
        style={{
          width: px * 1.2,
          height: Math.max(2, px * 0.1),
          borderRadius: 2,
          background: ACCENT,
          flex: "none",
        }}
      />
      <span>{text}</span>
    </Line>
  );
}

/** Copyright line of a Bible slide: unique, joined; shown when the theme asks for it. */
export function bibleCopyright(slide: Slide): string {
  const seen = new Set<string>();
  for (const c of slide.bible) if (c.copyright) seen.add(c.copyright);
  return [...seen].join(" · ");
}

export function footerLeft(slide: Slide, theme: Theme): string {
  if (!theme.showCopyright) return "";
  if (slide.bible.length > 0) return bibleCopyright(slide) || (slide.footer ?? "");
  return slide.footer ?? "";
}

/** One positioned text layer (what the transitions animate). */
export const TextFrame = forwardRef<
  HTMLDivElement,
  { frame: TextFrameData; box: Box; hidden?: boolean }
>(function TextFrame({ frame, box, hidden }, ref) {
  const { slide, theme, mode, px, outline } = frame;
  const left = footerLeft(slide, theme);
  const right = frame.position ?? "";
  const hasFooter = Boolean(left || right);
  const area = textArea(theme, box, hasFooter);
  const m = theme.margins;
  const f = theme.font;
  const bible = slide.bible.length > 0;

  const text: CSSProperties = {
    position: "absolute",
    inset: `${m.top}% ${m.right}% ${area.bottomPct}% ${m.left}%`,
    display: "flex",
    flexDirection: "column",
    justifyContent: "center",
    alignItems: bible && mode !== "bible-columns" ? "stretch" : undefined,
    fontSize: px,
    lineHeight: f.lineHeight,
    fontWeight: textWeight(theme, slide),
    fontStyle: f.italic ? "italic" : "normal",
    textTransform: f.uppercase ? "uppercase" : "none",
    textAlign: bible ? "left" : theme.align,
    textShadow: theme.shadow ?? undefined,
    letterSpacing: bible ? undefined : "-0.01em",
    WebkitTextStroke: outline ? `${outline.widthEm * px}px ${outline.color}` : undefined,
    paintOrder: outline ? "stroke fill" : undefined,
    textWrap: bible ? "pretty" : "balance",
    whiteSpace: "pre-line",
    visibility: hidden ? "hidden" : undefined,
  } as CSSProperties;

  let body: ReactNode = null;
  if (bible) {
    const cols = slide.bible;
    const first = cols[0];
    const header = first?.reference ?? slide.label ?? "";
    switch (mode) {
      case "bible-columns": {
        const gap = `${COLUMN_GAP * 100}%`;
        body = (
          <>
            <BibleHeader text={header} px={px} />
            <div
              style={{
                display: "grid",
                gridTemplateColumns: `repeat(${cols.length}, minmax(0, 1fr))`,
                columnGap: gap,
              }}
            >
              {cols.map((c, i) => (
                <div key={c.version} style={{ position: "relative" }}>
                  {i > 0 && (
                    <span
                      style={{
                        position: "absolute",
                        left: -(area.w * COLUMN_GAP) / 2 - 1,
                        top: 0,
                        bottom: 0,
                        width: 2,
                        background: "rgba(255,255,255,.12)",
                      }}
                    />
                  )}
                  <Line style={{ color: i === 0 ? undefined : "#C9D6EA" }}>{c.text}</Line>
                  <div style={{ height: px * 0.5 }} />
                  {versionLine(c, theme, px)}
                </div>
              ))}
            </div>
          </>
        );
        break;
      }
      case "bible-stacked":
      case "bible-subtitle": {
        const stacked = mode === "bible-stacked";
        body = (
          <>
            <BibleHeader text={header} px={px} />
            {cols.map((c, i) => (
              <Line
                key={c.version}
                style={
                  i === 0
                    ? undefined
                    : {
                        fontSize: px * (stacked ? 0.62 : SCALE.subtitleSecondary),
                        color: "#C9D6EA",
                        marginTop: px * (stacked ? 0.7 : 0.35),
                        paddingTop: stacked ? px * 0.5 : 0,
                        borderTop: stacked ? "2px solid rgba(255,255,255,.12)" : undefined,
                      }
                }
              >
                {c.text}
                {theme.showBibleVersionLabel && (
                  <span
                    style={{
                      font: `600 ${px * SCALE.versionLabel}px ${MONO}`,
                      color: ACCENT,
                      marginLeft: px * 0.35,
                      textShadow: "none",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {c.version}
                  </span>
                )}
              </Line>
            ))}
          </>
        );
        break;
      }
      default:
        body = (
          <>
            <BibleHeader text={header} px={px} />
            {first && <Line>{first.text}</Line>}
            <div style={{ height: px * 0.5 }} />
            {first && versionLine(first, theme, px)}
          </>
        );
    }
  } else {
    body = slide.lines.map((l, i) => (
      <Line key={i}>
        {l}
        {slide.translation[i] && (
          <div
            style={{
              fontSize: px * theme.translationScale,
              opacity: 0.85,
              fontWeight: Math.min(f.weight, 600),
            }}
          >
            {slide.translation[i]}
          </div>
        )}
      </Line>
    ));
  }

  const footerFont = Math.max(10, box.h * 0.0194);
  return (
    <div ref={ref} style={layerBase} data-slide-frame={frame.id}>
      <div style={text}>{body}</div>
      {hasFooter && (
        <div
          style={{
            position: "absolute",
            left: `${m.left}%`,
            right: `${m.right}%`,
            bottom: box.h * 0.039,
            display: "flex",
            justifyContent: "space-between",
            gap: footerFont * 2,
            font: `400 ${footerFont}px ${MONO}`,
            color: "rgba(255,255,255,.68)",
            textShadow: "none",
          }}
        >
          <span>{left}</span>
          <span style={{ whiteSpace: "nowrap" }}>{right}</span>
        </div>
      )}
    </div>
  );
});
