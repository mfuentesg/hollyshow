import type { Slide } from "@hollyshow/protocol";
import { useMemo, useState } from "react";
import { hasChords, parseChart, semitoneLabel, transposeChart } from "../lib/chords";
import { useT } from "../lib/i18n";
import { safeStorage } from "../lib/safeStorage";
import { findEntry } from "../lib/slideText";
import { useRemote } from "../state/RemoteContext";
import { Header, OfflineBanner } from "./common";

/** Lines of a slide with chords when available (`chart`), else the plain lyrics. */
export function chartLines(slide: Slide | null | undefined): string[] {
  if (!slide) return [];
  return slide.chart.length > 0 ? slide.chart : slide.lines;
}

function Chart({ lines, semitones, size }: { lines: string[]; semitones: number; size: number }) {
  return (
    <div className="chart" style={{ fontSize: size }}>
      {lines.map((raw, i) => {
        const line = transposeChart(raw, semitones);
        if (!hasChords(line)) {
          return (
            <div key={i} className="chart-line">
              <span className="seg-text">{line || " "}</span>
            </div>
          );
        }
        return (
          <div key={i} className="chart-line">
            {parseChart(line).map((seg, j) => (
              <span key={j} className="cseg">
                <span className="seg-chord mono">{seg.chord ?? " "}</span>
                <span className="seg-text">{seg.text || " "}</span>
              </span>
            ))}
          </div>
        );
      })}
    </div>
  );
}

export function Musician() {
  const t = useT();
  const { session, status } = useRemote();
  const [semitones, setSemitones] = useState(0);
  const [size, setSize] = useState(() => {
    const n = Number(safeStorage.get("hs.remote.musicianSize"));
    return n >= 14 && n <= 34 ? n : 20;
  });
  const view = session.view;
  const live = view?.live ?? null;
  const entry = view ? findEntry(session.show, live) : null;

  const next = useMemo(() => {
    if (!entry || !live) return null;
    const inSong = entry.slides[live.slideIndex + 1];
    if (inSong) return inSong;
    const idx = session.show.findIndex((e) => e.itemId === entry.itemId);
    return session.show[idx + 1]?.slides[0] ?? null;
  }, [entry, live, session.show]);

  const changeSize = (d: number) => {
    const n = Math.max(14, Math.min(34, size + d));
    setSize(n);
    safeStorage.set("hs.remote.musicianSize", String(n));
  };

  const lines = chartLines(live?.slide);
  const anyChords = lines.some(hasChords);

  return (
    <div className="screen">
      <Header
        title={entry?.title ?? t("mus.title")}
        status={status}
        serverName={session.serverName}
        role={session.role}
      />
      <OfflineBanner status={status} />
      <div className="toolbar" role="group" aria-label={t("mus.transpose")}>
        <span className="muted small">{t("mus.transpose")}</span>
        <button
          className="icon-btn"
          aria-label="-1"
          onClick={() => setSemitones((s) => Math.max(-11, s - 1))}
        >
          −
        </button>
        <span className="mono tr-val" aria-live="polite">
          {semitoneLabel(semitones)}
        </span>
        <button
          className="icon-btn"
          aria-label="+1"
          onClick={() => setSemitones((s) => Math.min(11, s + 1))}
        >
          +
        </button>
        <button className="chip" disabled={semitones === 0} onClick={() => setSemitones(0)}>
          {t("mus.reset")}
        </button>
        <span className="grow" />
        <button className="icon-btn" aria-label={t("crowd.smaller")} onClick={() => changeSize(-2)}>
          A−
        </button>
        <button className="icon-btn" aria-label={t("crowd.bigger")} onClick={() => changeSize(2)}>
          A+
        </button>
      </div>

      <div className="screen-body">
        <div className="scroll">
          {live ? (
            <>
              {live.slide.label && <div className="muted small">{live.slide.label}</div>}
              <Chart lines={lines} semitones={semitones} size={size} />
              {!anyChords && lines.length > 0 && <p className="muted small">{t("mus.noChords")}</p>}
            </>
          ) : (
            <p className="muted">{t("op.nothing")}</p>
          )}
          {next && (
            <div className="next-block">
              <div className="muted small">{t("mus.next")}</div>
              <Chart lines={chartLines(next)} semitones={semitones} size={Math.max(14, size - 4)} />
            </div>
          )}
          {session.show.length > 0 && (
            <details className="order">
              <summary>{t("mus.list")}</summary>
              <ol>
                {session.show.map((e) => (
                  <li key={e.itemId} className={e.itemId === live?.itemId ? "cur" : ""}>
                    {e.title}
                  </li>
                ))}
              </ol>
            </details>
          )}
        </div>
      </div>
    </div>
  );
}
