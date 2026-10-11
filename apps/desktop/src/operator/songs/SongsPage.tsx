import type { Song, SongSummary } from "@hollyshow/protocol";
import { Button, IconSearch } from "@hollyshow/ui";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { NavLink, useNavigate, useParams } from "react-router-dom";
import { useApi } from "../../api/context";
import { emptySong } from "../../lib/songText";
import { useOperator } from "../OperatorContext";
import { ImportDialog } from "./ImportDialog";
import { SongEditor } from "./SongEditor";

/** Song library (accent-insensitive search) next to the editor. */
export function SongsPage() {
  const { t } = useTranslation();
  const api = useApi();
  const nav = useNavigate();
  const op = useOperator();
  const { id } = useParams();
  const [q, setQ] = useState("");
  const [results, setResults] = useState<SongSummary[] | null>(null);
  const [song, setSong] = useState<Song | null>(null);
  const [importing, setImporting] = useState(false);
  const isNew = id === "new";

  useEffect(() => {
    if (!q.trim()) {
      setResults(null);
      return;
    }
    let alive = true;
    const timer = setTimeout(() => {
      void api
        .query({ kind: "searchSongs", text: q, limit: 200 })
        .then((r) => alive && setResults(r.items));
    }, 120);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [api, q]);

  useEffect(() => {
    setSong(null);
    if (!id) return;
    if (isNew) {
      setSong(emptySong(""));
      return;
    }
    let alive = true;
    void api.query({ kind: "getSong", id }).then((r) => alive && setSong(r.song));
    return () => {
      alive = false;
    };
  }, [api, id, isNew]);

  const list = useMemo(() => results ?? op.songs, [results, op.songs]);

  return (
    <div className="songs-page">
      <aside className="panel panel--left" aria-label={t("songs.library")}>
        <div className="row gap-8">
          <Button variant="primary" size="sm" onClick={() => nav("/operator/songs/new")}>
            + {t("songs.newSong")}
          </Button>
          <Button size="sm" onClick={() => setImporting(true)}>
            {t("songs.import")}
          </Button>
        </div>
        <label className="searchbox">
          <IconSearch />
          <input
            type="search"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder={t("songs.searchPlaceholder")}
            aria-label={t("songs.searchLabel")}
          />
        </label>
        <span className="muted small" aria-live="polite">
          {t("songs.results", { count: list.length })} · {t("songs.accentInsensitive")}
        </span>
        <ul className="song-list">
          {list.map((s) => (
            <li key={s.id}>
              <NavLink to={`/operator/songs/${encodeURIComponent(s.id)}`} className="song-row">
                <span className="song-row__title">{s.title}</span>
                <span className="muted small">
                  {[s.author, s.key, s.snippet ? `«…${s.snippet}…»` : null]
                    .filter(Boolean)
                    .join(" · ")}
                </span>
              </NavLink>
            </li>
          ))}
        </ul>
      </aside>
      <main className="center" aria-label={t("songs.editor")}>
        {song ? (
          <SongEditor key={isNew ? "new" : song.id} initial={song} isNew={isNew} />
        ) : (
          <p className="muted">{id ? t("common.loading") : t("songs.pickOne")}</p>
        )}
      </main>
      {importing && (
        <ImportDialog
          onClose={() => setImporting(false)}
          onImported={(imported) => {
            void op.reloadLibrary();
            if (imported[0]) nav(`/operator/songs/${encodeURIComponent(imported[0].id)}`);
          }}
        />
      )}
    </div>
  );
}
