import { SlideView, pickTheme, siblingSlides } from "@hollyshow/slide-renderer";
import { Logo } from "@hollyshow/ui";
import { useEffect, useMemo, useRef } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { useApi } from "../../api/context";
import { absoluteIndex, directionBetween, frameFor, nextSlideAfter, positionLabel } from "./frame";
import { useLivePresentation } from "./useLivePresentation";

/** Makes the page itself transparent (OBS browser source / capture with alpha). */
function useTransparentPage(on: boolean) {
  useEffect(() => {
    if (!on) return;
    const els = [document.documentElement, document.body];
    const prev = els.map((e) => e.style.background);
    els.forEach((e) => (e.style.background = "transparent"));
    return () => els.forEach((e, i) => (e.style.background = prev[i] ?? ""));
  }, [on]);
}

/**
 * Projector / OBS window (`/output/:id`). "Dumb" on purpose (PLAN §3.1.5): renders the last
 * received state, never depends on the operator window, hides the cursor and never shows
 * anything but the content (no chrome, no errors). `?transparent=1` or a lower-third output
 * draws without background so it can be composited.
 */
export function Output() {
  const { id = "main" } = useParams();
  const [search] = useSearchParams();
  const api = useApi();
  const live = useLivePresentation(api);

  const frame = live ? frameFor(live.view, id) : null;
  const transparent = search.get("transparent") === "1" || frame?.kind === "lowerThird";
  useTransparentPage(transparent);

  const cue = frame?.cue ?? null;
  const show = live?.show;
  const theme = useMemo(
    () => pickTheme(live?.themes ?? [], cue?.slide.themeId, frame?.themeId),
    [live?.themes, cue?.slide.themeId, frame?.themeId],
  );

  const abs = show ? absoluteIndex(show, cue) : null;
  const lastAbs = useRef<number | null>(null);
  const direction = directionBetween(lastAbs.current, abs);
  useEffect(() => {
    lastAbs.current = abs;
  });

  const siblings = useMemo(
    () => (cue && show ? siblingSlides(show, cue.itemId, cue.slide, theme) : undefined),
    [cue, show, theme],
  );
  const nextSlide = useMemo(() => (show ? nextSlideAfter(show, cue) : null), [show, cue]);
  const position = show && theme.showCopyright ? positionLabel(show, cue) : undefined;

  return (
    <div
      className="output-root"
      style={{
        cursor: "none",
        userSelect: "none",
        background: transparent ? "transparent" : "#000",
      }}
      onContextMenu={(e) => e.preventDefault()}
      data-output={id}
    >
      {frame && (
        <SlideView
          slide={cue?.slide ?? null}
          theme={theme}
          transparent={transparent}
          hideText={!frame.showText}
          blackout={frame.blackout}
          showLogo={frame.logo}
          logo={<Logo size={Math.round(Math.min(window.innerWidth, window.innerHeight) * 0.28)} />}
          background={frame.background}
          media={frame.media}
          overlay={frame.overlay}
          siblings={siblings}
          nextSlide={nextSlide}
          direction={direction}
          position={position}
        />
      )}
    </div>
  );
}
