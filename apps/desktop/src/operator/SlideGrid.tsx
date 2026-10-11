import type { Slide } from "@hollyshow/protocol";
import { SlideView } from "@hollyshow/slide-renderer";
import { Button, Select } from "@hollyshow/ui";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { useApi } from "../api/context";
import { cmd } from "../lib/cmd";
import { sectionKey } from "../lib/sections";
import { LiveEditDialog } from "./LiveEditDialog";
import { useOperator } from "./OperatorContext";
import { selectedEntry } from "./selection";
import type { Song } from "@hollyshow/protocol";

/** Slides of the selected service item with live / preview states and section keys. */
export function SlideGrid() {
  const { t } = useTranslation();
  const api = useApi();
  const nav = useNavigate();
  const op = useOperator();
  const { p, playlist, songs, themeFor, editPlaylist, send } = op;
  const { view } = p;
  const [editing, setEditing] = useState(false);
  const [focusIdx, setFocusIdx] = useState<number | null>(null);

  const entry = selectedEntry(op);
  const item = playlist?.items.find((i) => i.id === entry?.itemId) ?? null;
  const songId = item?.content.kind === "song" ? item.content.songId : null;
  const summary = songId ? songs.find((s) => s.id === songId) : undefined;
  const [song, setSong] = useState<Song | null>(null);

  useEffect(() => {
    setSong(null);
    if (!songId) return;
    let alive = true;
    void api.query({ kind: "getSong", id: songId }).then((r) => alive && setSong(r.song));
    return () => {
      alive = false;
    };
  }, [api, songId, op.p.view.showRevision]);

  useEffect(() => setFocusIdx(null), [entry?.itemId]);

  if (!entry) {
    return (
      <main className="center" aria-label={t("slides.title")}>
        <p className="muted">{t("slides.empty")}</p>
      </main>
    );
  }

  const liveIdx = view.live?.itemId === entry.itemId ? view.live.slideIndex : -1;
  const prevIdx = view.preview?.itemId === entry.itemId ? view.preview.slideIndex : -1;
  const editIdx = focusIdx ?? (liveIdx >= 0 ? liveIdx : prevIdx >= 0 ? prevIdx : 0);
  const editSlide: Slide | undefined = entry.slides[editIdx];
  const content = item?.content;
  // Next / Enter / arrows act on the live (else preview) cursor, not on the browsed item.
  const actingId = view.live?.itemId ?? view.preview?.itemId ?? null;
  const actingEntry = actingId ? (op.p.show.find((e) => e.itemId === actingId) ?? null) : null;

  return (
    <main className="center" aria-label={t("slides.title")}>
      <div className="center__head">
        <h1>{entry.title}</h1>
        <span className="muted small">
          {[summary?.author, song?.copyright].filter(Boolean).join(" · ")}
        </span>
        <div className="row gap-8 wrap push-right">
          {content?.kind === "song" && song && (
            <label className="select-chip">
              <span className="hs-sr-only">{t("service.arrangement")}</span>
              <Select
                className="hs-btn--sm"
                aria-label={t("service.arrangement")}
                value={content.arrangementId ?? ""}
                onChange={(e) =>
                  editPlaylist((pl) => ({
                    ...pl,
                    items: pl.items.map((i) =>
                      i.id === item!.id && i.content.kind === "song"
                        ? { ...i, content: { ...i.content, arrangementId: e.target.value || null } }
                        : i,
                    ),
                  }))
                }
              >
                <option value="">
                  {t("slides.arrangement", { name: t("service.arrangementDefault") })}
                </option>
                {song.arrangements.map((a) => (
                  <option key={a.id} value={a.id}>
                    {t("slides.arrangement", { name: a.name })}
                  </option>
                ))}
              </Select>
            </label>
          )}
          {item && (
            <Select
              className="hs-btn--sm"
              aria-label={t("service.theme")}
              value={item.themeId ?? ""}
              onChange={(e) =>
                editPlaylist((pl) => ({
                  ...pl,
                  items: pl.items.map((i) =>
                    i.id === item.id ? { ...i, themeId: e.target.value || null } : i,
                  ),
                }))
              }
            >
              <option value="">{t("slides.theme", { name: t("service.themeDefault") })}</option>
              {op.themes.map((th) => (
                <option key={th.id} value={th.id}>
                  {t("slides.theme", { name: th.name })}
                </option>
              ))}
            </Select>
          )}
          {songId && (
            <Button size="sm" onClick={() => nav(`/operator/songs/${encodeURIComponent(songId)}`)}>
              {t("common.edit")}
            </Button>
          )}
          <Button
            size="sm"
            disabled={view.serviceMode || !editSlide}
            title={view.serviceMode ? t("errors.locked") : t("slides.liveEditHint")}
            onClick={() => setEditing(true)}
          >
            {t("slides.liveEdit")}
          </Button>
        </div>
      </div>

      {actingEntry && actingEntry.itemId !== entry.itemId && (
        <div className="browse-banner" role="status">
          <span>
            {t("slides.browsing", { name: entry.title })} ·{" "}
            <strong>{t("slides.actingOn", { name: actingEntry.title })}</strong>
          </span>
          <Button size="sm" onClick={() => void send(cmd.goTo(entry.itemId, 0, "preview"))}>
            {t("slides.loadPreview")}
          </Button>
        </div>
      )}

      {entry.slides.length === 0 ? (
        <p className="muted">{t("slides.noSlides")}</p>
      ) : (
        <ul className="grid" aria-label={t("slides.title")}>
          {entry.slides.map((s, i) => {
            const live = i === liveIdx;
            const prev = i === prevIdx;
            const key = sectionKey(s.label);
            const text = s.bible.length
              ? s.bible.map((c) => c.text).join(" / ")
              : s.lines.join(" ");
            return (
              <li key={s.id}>
                <button
                  type="button"
                  data-slide-tile
                  className={["tile", live && "is-live", prev && "is-preview"]
                    .filter(Boolean)
                    .join(" ")}
                  aria-label={t("slides.tileLabel", {
                    n: i + 1,
                    section: s.label ?? "",
                    text,
                    state: live ? t("operator.live") : prev ? t("operator.preview") : "",
                  })}
                  aria-current={live ? "true" : undefined}
                  onClick={() => {
                    setFocusIdx(i);
                    void send(cmd.goTo(entry.itemId, i, "preview"));
                  }}
                  onDoubleClick={() => void send(cmd.goTo(entry.itemId, i, "live"))}
                >
                  <span className="tile__thumb">
                    <SlideView slide={s} theme={themeFor(s.themeId ?? item?.themeId)} />
                  </span>
                  <span className="tile__meta">
                    <span className={live ? "tag-live" : prev ? "tag-preview" : "tag-none"}>
                      {live
                        ? `● ${t("slides.stateLive")}`
                        : prev
                          ? t("operator.preview").toUpperCase()
                          : "·"}
                    </span>
                    <span className="muted">
                      <span className="mono key">{key ?? i + 1}</span> ·{" "}
                      {s.label ?? t("slides.slideN", { n: i + 1 })}
                    </span>
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}

      <div className="hints" aria-label={t("shortcuts.title")}>
        <span className="hs-kbd">← → {t("shortcuts.slide")}</span>
        <span className="hs-kbd">V1 V2 C P F — {t("shortcuts.jump")}</span>
        <span className="hs-kbd">Enter — {t("shortcuts.previewToLive")}</span>
        <span className="hs-kbd">B — {t("shortcuts.black")}</span>
        <span className="hs-kbd">? — {t("shortcuts.title")}</span>
      </div>

      {editing && editSlide && (
        <LiveEditDialog
          itemId={entry.itemId}
          slideIndex={editIdx}
          slide={editSlide}
          onClose={() => setEditing(false)}
        />
      )}
    </main>
  );
}
