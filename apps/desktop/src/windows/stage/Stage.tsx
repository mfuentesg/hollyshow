import { fitFontSize, canvasMeasure, useBox, useFontEpoch } from "@hollyshow/slide-renderer";
import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "../../api/context";
import { useLivePresentation } from "../output/useLivePresentation";
import { registerStageStrings, STAGE_NS } from "./i18n";
import {
  arrangementStrip,
  formatClock,
  formatDuration,
  kindKey,
  nextCue,
  primaryCountdown,
  stageRows,
  type Urgency,
} from "./stageModel";

registerStageStrings();

const MONO = "var(--hs-font-mono, 'JetBrains Mono', ui-monospace, monospace)";
const PROJ = "var(--hs-font-projection, Archivo, sans-serif)";
const CHORDS_KEY = "hollyshow.stage.chords";

/** Design is 1280x720; every size is multiplied by `--u` (= viewport height / 720). */
const u = (n: number) => `calc(var(--u) * ${n})`;

const URGENCY_COLOR: Record<Urgency, string> = {
  ok: "var(--hs-ok, #2FA36B)",
  warn: "var(--hs-amber, #F2B544)",
  over: "#FF6369",
};

function readChords(): boolean {
  try {
    const q = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get("chords");
    if (q !== null) return q !== "0";
    return localStorage.getItem(CHORDS_KEY) !== "0";
  } catch {
    return true;
  }
}

function useNow(intervalMs = 250): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const i = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(i);
  }, [intervalMs]);
  return now;
}

/**
 * Stage display (`/stage`), docs/design/Stage.dc.html: clock, countdown, arrangement strip,
 * message banner, current slide (with optional chords) and the next slide.
 * Like the projector it keeps the last state and never blanks on connection problems.
 */
export function Stage() {
  const { t } = useTranslation(STAGE_NS);
  const live = useLivePresentation(useApi());
  const now = useNow();
  const epoch = useFontEpoch();
  const [chords, setChords] = useState(readChords);
  const [curRef, curBox] = useBox();

  const toggleChords = () =>
    setChords((c) => {
      try {
        localStorage.setItem(CHORDS_KEY, c ? "0" : "1");
      } catch {
        /* storage may be blocked; the toggle still works for this session */
      }
      return !c;
    });

  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === "c" && !e.ctrlKey && !e.metaKey && !e.altKey) toggleChords();
    };
    window.addEventListener("keydown", on);
    return () => window.removeEventListener("keydown", on);
  }, []);

  const view = live?.view;
  const show = live?.show;
  const cue = view?.live ?? null;
  const next = useMemo(
    () => (show && view ? nextCue(show, cue, view.preview) : null),
    [show, view, cue],
  );
  const strip = useMemo(() => (show ? arrangementStrip(show, cue, next) : []), [show, cue, next]);
  const rows = useMemo(() => (cue ? stageRows(cue.slide, chords) : []), [cue, chords]);
  const countdown = view ? primaryCountdown(view.itemClock, view.timers, now) : null;
  const title = cue?.itemId
    ? show?.find((e) => e.itemId === cue.itemId)?.title
    : (cue?.slide.label ?? undefined);

  // Auto-fit the current slide: chord rows count as 0.52x lines (monospace, smaller).
  const unit = curBox.h / 720;
  const fontPx = useMemo(() => {
    if (rows.length === 0) return 50 * unit;
    const lines = rows.flatMap((r) => (r.chords !== null ? [r.chords, r.lyric] : [r.lyric]));
    const scales = rows.flatMap((r) => (r.chords !== null ? [0.52, 1] : [1]));
    return fitFontSize({
      lines,
      scales,
      width: curBox.w,
      height: Math.max(40, curBox.h - 40 * unit),
      minPx: 26 * unit,
      maxPx: 84 * unit,
      lineHeight: 1.15,
      measure: canvasMeasure("Archivo", 800),
    });
    // epoch: re-measure once the web fonts arrive.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, curBox.w, curBox.h, unit, epoch]);

  const root: CSSProperties = {
    ["--u" as string]: "calc(100vh / 720)",
    width: "100vw",
    height: "100vh",
    boxSizing: "border-box",
    padding: `${u(28)} ${u(36)}`,
    display: "flex",
    flexDirection: "column",
    gap: u(20),
    background: "#000",
    color: "#fff",
    fontFamily: "var(--hs-font-ui, 'Instrument Sans', sans-serif)",
    overflow: "hidden",
    cursor: "none",
    userSelect: "none",
  };

  const countdownText = countdown
    ? countdown.stopwatch
      ? t("elapsed", { time: formatDuration(countdown.ms) })
      : countdown.ms < 0
        ? t("over", { time: formatDuration(-countdown.ms) })
        : t("remaining", { time: formatDuration(countdown.ms) })
    : null;

  return (
    <div style={root} data-window="stage">
      <header style={{ display: "flex", alignItems: "center", gap: u(24) }}>
        <span style={{ font: `700 ${u(44)} ${MONO}` }}>{formatClock(now)}</span>
        <div style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
          <span style={{ fontSize: u(14), color: "var(--hs-text-secondary, #9AA3B2)" }}>
            {cue ? `${t(`kind.${kindKey(cue.slide)}`)}${title ? ` · ${title}` : ""}` : t("empty")}
          </span>
          {countdown && countdownText && (
            <span
              style={{
                font: `700 ${u(22)} ${MONO}`,
                color: URGENCY_COLOR[countdown.urgency],
              }}
            >
              {countdown.label ? `${countdown.label} · ` : ""}
              {countdownText}
            </span>
          )}
        </div>
        <div style={{ marginLeft: "auto", display: "flex", gap: u(8), alignItems: "center" }}>
          {view?.blackout && (
            <span
              style={{
                font: `700 ${u(14)} ${MONO}`,
                padding: `${u(6)} ${u(10)}`,
                borderRadius: u(8),
                border: "2px solid #FF6369",
                color: "#FF6369",
                letterSpacing: 1,
              }}
            >
              {t("blackout")}
            </span>
          )}
          {strip.map((k) => (
            <span
              key={k.index}
              data-state={k.state}
              style={{
                minWidth: u(48),
                height: u(40),
                display: "inline-flex",
                alignItems: "center",
                justifyContent: "center",
                padding: `0 ${u(10)}`,
                borderRadius: u(8),
                boxSizing: "border-box",
                font: `700 ${u(18)} ${MONO}`,
                ...(k.state === "live"
                  ? { background: "#D13438", color: "#fff" }
                  : k.state === "next"
                    ? { border: "2px solid #5B9BFF", color: "#8DB8FF" }
                    : { border: "1px solid #3A404D", color: "#9AA3B2" }),
              }}
            >
              {k.key}
            </span>
          ))}
          <button
            type="button"
            aria-pressed={chords}
            onClick={toggleChords}
            style={{
              cursor: "pointer",
              marginLeft: u(8),
              height: u(40),
              padding: `0 ${u(12)}`,
              borderRadius: u(8),
              font: `600 ${u(14)} ${MONO}`,
              background: chords ? "var(--hs-amber, #F2B544)" : "transparent",
              color: chords ? "var(--hs-on-amber, #1A1405)" : "#9AA3B2",
              border: chords ? "none" : "1px solid #3A404D",
            }}
          >
            {t("chords")}
          </button>
        </div>
      </header>

      {view?.stageMessage && (
        <div
          role="alert"
          style={{
            display: "flex",
            alignItems: "center",
            gap: u(12),
            padding: `${u(12)} ${u(18)}`,
            borderRadius: u(12),
            background: "var(--hs-amber, #F2B544)",
            color: "var(--hs-on-amber, #1A1405)",
            fontWeight: 700,
            fontSize: u(24),
          }}
        >
          <svg
            width={26}
            height={26}
            viewBox="0 0 24 24"
            fill="none"
            stroke="#1A1405"
            strokeWidth="2.2"
            aria-hidden="true"
            style={{ flex: "none" }}
          >
            <path d="M4 5h16v11H8l-4 4z" />
          </svg>
          <span style={{ overflowWrap: "anywhere" }}>{view.stageMessage.text}</span>
        </div>
      )}

      <section
        ref={curRef}
        style={{
          flex: 1,
          minHeight: 0,
          display: "flex",
          flexDirection: "column",
          justifyContent: "center",
          gap: u(8),
        }}
      >
        {cue && (
          <>
            <span
              style={{ fontSize: u(16), fontWeight: 700, letterSpacing: 1.5, color: "#FF8A8E" }}
            >
              ● {t("now")}
              {cue.slide.label ? ` · ${cue.slide.label.toUpperCase()}` : ""}
            </span>
            <div style={{ fontFamily: PROJ, fontWeight: 800, fontSize: fontPx, lineHeight: 1.15 }}>
              {rows.map((r, i) => (
                <div key={i}>
                  {r.chords !== null && (
                    <div
                      style={{
                        font: `700 ${fontPx * 0.52}px ${MONO}`,
                        color: "var(--hs-amber, #F2B544)",
                        whiteSpace: "pre",
                      }}
                    >
                      {r.chords}
                    </div>
                  )}
                  <div style={{ overflowWrap: "anywhere" }}>{r.lyric}</div>
                </div>
              ))}
            </div>
          </>
        )}
      </section>

      <footer
        style={{
          display: "flex",
          flexDirection: "column",
          gap: u(6),
          borderTop: "2px solid #2A2F3A",
          paddingTop: u(16),
          minHeight: u(80),
        }}
      >
        {next && (
          <>
            <span
              style={{ fontSize: u(14), fontWeight: 700, letterSpacing: 1.5, color: "#8DB8FF" }}
            >
              {t("next")}
              {next.slide.label ? ` · ${next.slide.label.toUpperCase()}` : ""}
            </span>
            <div
              style={{
                fontFamily: PROJ,
                fontWeight: 600,
                fontSize: u(28),
                lineHeight: 1.25,
                color: "#B8C2D3",
                display: "-webkit-box",
                WebkitLineClamp: 3,
                WebkitBoxOrient: "vertical",
                overflow: "hidden",
              }}
            >
              {stageRowsPlain(next.slide)}
            </div>
          </>
        )}
      </footer>
    </div>
  );
}

function stageRowsPlain(slide: Parameters<typeof stageRows>[0]): string {
  return stageRows(slide, false)
    .map((r) => r.lyric)
    .join(" ");
}
