import type { MediaId, Slide, Theme, TransitionSpec } from "@hollyshow/protocol";
import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { BackgroundLayer } from "./BackgroundLayer";
import { cachedGroupPx, cachedPlan, slideSignature } from "./fitCache";
import { useBox, useFontEpoch, useIsoLayoutEffect, usePrefersReducedMotion } from "./hooks";
import type { Box } from "./layout";
import { defaultMediaResolver, preloadMedia, type MediaResolver } from "./media";
import { MediaLayer, OverlayBar } from "./MediaLayer";
import { TextFrame, type Outline, type TextFrameData } from "./TextFrame";
import { buildTransition, playTransition, type Direction } from "./transitions";

export interface SlideViewProps {
  slide: Slide | null;
  theme: Theme;
  /** Hide the text but keep the background ("clear"). */
  hideText?: boolean;
  /** Draws no background so the output can be composited (OBS / lower third). */
  transparent?: boolean;
  className?: string;
  style?: CSSProperties;

  /** Next slide: its fit is computed and its media preloaded while the current one is on screen. */
  nextSlide?: Slide | null;
  /** Slides that share one font size with `slide` (song-wide size); see `siblingSlides`. */
  siblings?: Slide[];
  /** Forces the base font size (px) instead of auto-fitting. */
  fontPx?: number;
  /** Per-output layers (OutputLayers). */
  background?: MediaId | null;
  media?: MediaId | null;
  overlay?: string | null;
  /** Fades everything to black (or to transparent when `transparent`). */
  blackout?: boolean;
  /** Logo content and its flag; the logo replaces text/media and keeps the background. */
  logo?: ReactNode;
  showLogo?: boolean;
  resolveMedia?: MediaResolver;
  /** Overrides `theme.transition`. */
  transition?: TransitionSpec;
  /** Slide direction, used by the `slide` transition. */
  direction?: Direction;
  /** `thumbnail`: static (no video, Ken Burns, overlay or transitions). */
  mode?: "live" | "thumbnail";
  /** Lays out for this fixed size instead of observing the element (thumbnails). */
  fixedBox?: Box;
  /** Right-hand footer text such as "V1 · 2/3". */
  position?: string;
  outline?: Outline | null;
  /** When true the OS "reduce motion" setting turns transitions into cuts. */
  respectReducedMotion?: boolean;
}

const fillStyle: CSSProperties = { position: "absolute", inset: 0 };
const fade = (ms: number): CSSProperties => ({ transition: `opacity ${ms}ms ease-in-out` });

/**
 * The single slide renderer shared by output, preview, thumbnails, stage and OBS (PLAN §3.1.3).
 * Fills its parent; text is auto-fitted inside the theme's safe area; changing slides plays the
 * theme transition on the text layer (background layer changes are handled independently so a
 * song's shared background never flickers between slides).
 */
export function SlideView(props: SlideViewProps) {
  const {
    slide,
    theme,
    hideText,
    transparent,
    className,
    style,
    nextSlide,
    siblings,
    fontPx,
    background = null,
    media = null,
    overlay = null,
    blackout = false,
    logo,
    showLogo = false,
    resolveMedia = defaultMediaResolver,
    direction = "forward",
    mode = "live",
    fixedBox,
    position,
    outline,
    respectReducedMotion = false,
  } = props;
  const spec = props.transition ?? theme.transition;
  const live = mode === "live";
  const [rootRef, box] = useBox(fixedBox);
  const epoch = useFontEpoch();
  const reduced = usePrefersReducedMotion();
  const animated = live && !(respectReducedMotion && reduced);

  // --- fit -------------------------------------------------------------------------------
  const sig = slide ? slideSignature(slide) : "";
  const groupKey = siblings ? siblings.map((s) => s.id).join("|") : "";
  const frame = useMemo<TextFrameData | null>(() => {
    if (!slide) return null;
    const plan = cachedPlan(slide, theme, box, epoch);
    let px = plan.px;
    if (fontPx !== undefined) px = fontPx;
    else if (siblings && siblings.length > 1)
      px = Math.min(px, cachedGroupPx(siblings, theme, box, epoch));
    return { id: slide.id, slide, theme, mode: plan.mode, px, position, outline };
    // `sig`/`groupKey` stand in for deep slide/sibling identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sig, groupKey, slide?.id, theme, box, epoch, fontPx, position, outline]);

  // --- transitions -----------------------------------------------------------------------
  const curEl = useRef<HTMLDivElement>(null);
  const outEl = useRef<HTMLDivElement>(null);
  const prevFrame = useRef<TextFrameData | null>(frame);
  const tokenRef = useRef(0);
  const [out, setOut] = useState<{ frame: TextFrameData; token: number } | null>(null);
  const frameId = frame ? `${frame.id}@${theme.id}` : "";

  useIsoLayoutEffect(() => {
    const last = prevFrame.current;
    if (animated && last && frame && spec.kind !== "cut" && spec.durationMs > 0) {
      setOut({ frame: last, token: ++tokenRef.current });
    } else {
      setOut(null);
    }
    // Only a change of slide (not an in-place edit) starts a transition.
  }, [frameId]);
  useIsoLayoutEffect(() => {
    prevFrame.current = frame;
  });

  const outToken = out?.token;
  useIsoLayoutEffect(() => {
    if (outToken === undefined) return;
    const cur = curEl.current;
    const old = outEl.current;
    if (!cur || !old) {
      setOut(null);
      return;
    }
    const plan = buildTransition(spec, direction);
    const handle = playTransition(old, cur, {
      plan,
      lineByLine: spec.lineByLine,
      lines: Array.from(cur.querySelectorAll("[data-line]")),
    });
    let cancelled = false;
    void handle.finished.then(() => {
      if (!cancelled) setOut((o) => (o?.token === outToken ? null : o));
    });
    return () => {
      cancelled = true;
      handle.cancel();
    };
  }, [outToken]);

  // --- pre-render of the next slide ------------------------------------------------------
  const nextSig = nextSlide ? slideSignature(nextSlide) : "";
  useEffect(() => {
    if (!live || !nextSlide) return;
    const run = () => {
      cachedPlan(nextSlide, theme, box, epoch);
      preloadMedia([nextSlide.media], resolveMedia);
    };
    const id = setTimeout(run, 0);
    return () => clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nextSig, nextSlide?.id, theme, box, epoch, live]);

  const root: CSSProperties = {
    position: "relative",
    width: "100%",
    height: "100%",
    overflow: "hidden",
    background: transparent ? "transparent" : "#000",
    color: theme.textColor,
    fontFamily: `${theme.font.family}, var(--hs-font-projection, sans-serif)`,
    contain: "strict",
    ...style,
  };
  const fadeMs = animated ? Math.min(400, Math.max(120, spec.durationMs)) : 0;
  const mediaId = media ?? slide?.media ?? null;
  const showText = Boolean(frame) && !hideText;

  return (
    <div
      ref={rootRef}
      className={className}
      style={root}
      data-slide-id={slide?.id ?? ""}
      data-mode={mode}
    >
      <div style={{ ...fillStyle, opacity: blackout ? 0 : 1, ...fade(fadeMs || 0) }}>
        {!transparent && (
          <div style={fillStyle}>
            <BackgroundLayer
              spec={{ bg: theme.background, overrideId: background }}
              resolve={resolveMedia}
              transition={spec}
              live={animated}
              transitionKey={frameId}
            />
          </div>
        )}
        <div style={{ ...fillStyle, opacity: showLogo ? 0 : 1, ...fade(fadeMs) }}>
          {mediaId && (
            <MediaLayer id={mediaId} resolve={resolveMedia} live={animated} fadeMs={fadeMs} />
          )}
          <div style={{ ...fillStyle, opacity: showText ? 1 : 0, ...fade(fadeMs) }}>
            {frame && <TextFrame ref={curEl} frame={frame} box={box} />}
            {out && <TextFrame key={out.token} ref={outEl} frame={out.frame} box={box} />}
          </div>
          {live && overlay && <OverlayBar text={overlay} boxH={box.h} fadeMs={fadeMs} />}
        </div>
        {logo && (
          <div
            style={{
              ...fillStyle,
              display: "grid",
              placeItems: "center",
              opacity: showLogo ? 1 : 0,
              pointerEvents: "none",
              ...fade(fadeMs),
            }}
          >
            {logo}
          </div>
        )}
      </div>
    </div>
  );
}
