import type {
  MediaCategory,
  MediaLibraryItem,
  MediaResult,
  MediaSearchResponse,
  MediaSourceFilter,
} from "@hollyshow/protocol";
import { Button, Chip, IconSearch, Select, TextInput } from "@hollyshow/ui";
import { useCallback, useEffect, useRef, useState } from "react";
import { useApi } from "../../api/context";
import { ImagePreviewModal } from "./ImagePreviewModal";
import { LicenseBadge } from "./LicenseBadge";
import { fileSrc, saveTextFile } from "./files";
import { useMediaT } from "./strings";
import "./media.css";

export interface ImageSearchPanelProps {
  /** Called with the stored image when the operator picks "Usar como fondo" (download happens first). */
  onUseAsBackground?: (item: MediaLibraryItem) => void;
}

type Tab = "search" | "library";

/** "Buscar fondos": search, preview, download and reuse of openly licensed background images. */
export function ImageSearchPanel({ onUseAsBackground }: ImageSearchPanelProps) {
  const api = useApi();
  const { t, lang } = useMediaT();
  const available = Boolean(api.mediaSearch && api.mediaDownload && api.mediaList);

  const [tab, setTab] = useState<Tab>("search");
  const [categories, setCategories] = useState<MediaCategory[]>([]);
  const [category, setCategory] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [source, setSource] = useState<MediaSourceFilter>("all");
  const [nc, setNc] = useState(false);
  const [resp, setResp] = useState<MediaSearchResponse | null>(null);
  const [results, setResults] = useState<MediaResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [library, setLibrary] = useState<MediaLibraryItem[]>([]);
  const [selected, setSelected] = useState<MediaResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const seq = useRef(0);
  const lastQuery = useRef("");

  useEffect(() => {
    if (!available) return;
    void api.mediaCategories?.().then(setCategories, () => {});
    void api.mediaList?.().then(setLibrary, () => {});
  }, [api, available]);

  const run = useCallback(
    async (query: string, page: number, opts: { source: MediaSourceFilter; nc: boolean }) => {
      if (!api.mediaSearch || !query.trim()) return;
      const mine = ++seq.current;
      setLoading(true);
      setError(null);
      try {
        const r = await api.mediaSearch({
          query,
          source: opts.source,
          page,
          allowNoncommercial: opts.nc,
        });
        if (mine !== seq.current) return;
        lastQuery.current = query;
        setResp(r);
        setResults((prev) => (page > 1 ? [...prev, ...r.results] : r.results));
      } catch (e) {
        if (mine === seq.current) setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (mine === seq.current) setLoading(false);
      }
    },
    [api],
  );

  const submit = (q = text, opts = { source, nc }) => void run(q, 1, opts);

  const pickCategory = (c: MediaCategory) => {
    setCategory(c.id);
    setText(c.queryEs);
    submit(c.queryEs);
  };

  const libraryFor = (r: MediaResult) =>
    library.find((i) => i.id === `${r.source}-${r.id}`) ?? null;

  const download = async (r: MediaResult): Promise<MediaLibraryItem | null> => {
    if (!api.mediaDownload) return null;
    setBusy(true);
    setStatus(null);
    try {
      const item = await api.mediaDownload(r, nc);
      setLibrary((l) => [item, ...l.filter((x) => x.id !== item.id)]);
      setStatus(t("downloadedToast"));
      return item;
    } catch (e) {
      setStatus(t("downloadFailed", { message: e instanceof Error ? e.message : String(e) }));
      return null;
    } finally {
      setBusy(false);
    }
  };

  const applyResult = async (r: MediaResult) => {
    const item = libraryFor(r) ?? (await download(r));
    if (item) {
      onUseAsBackground?.(item);
      setSelected(null);
    }
  };

  const remove = async (item: MediaLibraryItem) => {
    if (!api.mediaRemove) return;
    try {
      await api.mediaRemove(item.id);
      setLibrary((l) => l.filter((x) => x.id !== item.id));
    } catch (e) {
      setStatus(e instanceof Error ? e.message : String(e));
    }
  };

  const exportCredits = async () => {
    if (!api.mediaCredits) return;
    try {
      const md = await api.mediaCredits();
      if (saveTextFile("creditos-imagenes.md", md)) setStatus(t("creditsExported"));
    } catch (e) {
      setStatus(e instanceof Error ? e.message : String(e));
    }
  };

  if (!available) {
    return (
      <section className="media-panel" aria-label={t("title")}>
        <p className="media-note">{t("unavailable")}</p>
      </section>
    );
  }

  return (
    <section className="media-panel" aria-label={t("title")}>
      <header className="media-panel__head">
        <h2>{t("title")}</h2>
        <div className="media-tabs" role="group" aria-label={t("title")}>
          <button type="button" aria-pressed={tab === "search"} onClick={() => setTab("search")}>
            {t("tabSearch")}
          </button>
          <button type="button" aria-pressed={tab === "library"} onClick={() => setTab("library")}>
            {t("tabLibrary")} ({library.length})
          </button>
        </div>
        <Button size="sm" onClick={() => void exportCredits()} disabled={library.length === 0}>
          {t("exportCredits")}
        </Button>
        <p className="media-panel__sub">{t("subtitle")}</p>
      </header>

      {status && (
        <p className="media-note" role="status">
          {status}
        </p>
      )}

      {tab === "search" ? (
        <>
          <form
            className="media-search"
            role="search"
            onSubmit={(e) => {
              e.preventDefault();
              setCategory(null);
              submit();
            }}
          >
            <TextInput
              type="search"
              aria-label={t("searchLabel")}
              placeholder={t("searchPlaceholder")}
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
            <Button type="submit" variant="primary" disabled={loading || !text.trim()}>
              <IconSearch aria-hidden /> {t("search")}
            </Button>
          </form>

          <div className="media-options">
            <label>
              {t("source")}
              <Select
                value={source}
                onChange={(e) => {
                  const s = e.target.value as MediaSourceFilter;
                  setSource(s);
                  if (lastQuery.current) submit(lastQuery.current, { source: s, nc });
                }}
              >
                <option value="all">{t("sourceAll")}</option>
                <option value="openverse">{t("sourceOpenverse")}</option>
                <option value="commons">{t("sourceCommons")}</option>
              </Select>
            </label>
            <label>
              <input
                type="checkbox"
                checked={nc}
                onChange={(e) => {
                  setNc(e.target.checked);
                  if (lastQuery.current)
                    submit(lastQuery.current, { source, nc: e.target.checked });
                }}
              />
              {t("noncommercial")}
            </label>
          </div>

          <ul className="media-chips" aria-label={t("categories")}>
            {categories.map((c) => (
              <li key={c.id}>
                <Chip pressed={category === c.id} onClick={() => pickCategory(c)}>
                  {lang === "en" ? c.labelEn : c.labelEs}
                </Chip>
              </li>
            ))}
          </ul>

          {resp && resp.translatedQuery && (
            <p className="media-note">{t("translated", { q: resp.translatedQuery })}</p>
          )}
          {error && (
            <p className="media-banner" role="alert">
              {error}
            </p>
          )}
          {resp?.failures.map((f) => (
            <p className="media-banner" role="alert" key={f.source}>
              {f.retryAfterSecs != null
                ? t("rateLimited", {
                    source: f.source === "openverse" ? "Openverse" : "Wikimedia Commons",
                    s: f.retryAfterSecs,
                  })
                : t("failure", {
                    source: f.source === "openverse" ? "Openverse" : "Wikimedia Commons",
                    message: f.message,
                  })}
            </p>
          ))}

          {loading && results.length === 0 && (
            <p className="media-note" role="status">
              {t("loading")}
            </p>
          )}
          {!resp && !loading && <p className="media-note">{t("hint")}</p>}
          {resp && !loading && results.length === 0 && <p className="media-note">{t("empty")}</p>}

          {results.length > 0 && (
            <ul className="media-grid" aria-label={t("results")}>
              {results.map((r) => (
                <li key={`${r.source}-${r.id}`}>
                  <button
                    type="button"
                    className="media-card"
                    aria-label={t("open", { title: r.title })}
                    onClick={() => setSelected(r)}
                  >
                    <img
                      className="media-card__img"
                      src={fileSrc(r.thumbUrl)}
                      alt=""
                      loading="lazy"
                    />
                    <LicenseBadge license={r.license} />
                    {libraryFor(r) && (
                      <span className="license-badge license-badge--free media-card__done">
                        {t("downloaded")}
                      </span>
                    )}
                    <span className="media-card__title">{r.title}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          {resp?.hasMore && (
            <Button
              disabled={loading}
              onClick={() => void run(lastQuery.current, (resp?.page ?? 1) + 1, { source, nc })}
            >
              {loading ? t("loading") : t("loadMore")}
            </Button>
          )}
        </>
      ) : library.length === 0 ? (
        <p className="media-note">{t("libraryEmpty")}</p>
      ) : (
        <ul className="media-grid" aria-label={t("library")}>
          {library.map((item) => (
            <li key={item.id}>
              <div className="media-card" style={{ cursor: "default" }}>
                <img
                  className="media-card__img"
                  src={fileSrc(item.thumbPath)}
                  alt={item.result.title}
                  loading="lazy"
                />
                <LicenseBadge license={item.result.license} />
                <span className="media-card__title">{item.result.title}</span>
                <div className="media-actions" style={{ padding: "0 8px 8px" }}>
                  <Button size="sm" variant="primary" onClick={() => onUseAsBackground?.(item)}>
                    {t("useAsBackground")}
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    onClick={() => void remove(item)}
                    aria-label={`${t("remove")} ${item.result.title}`}
                  >
                    {t("remove")}
                  </Button>
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}

      {selected && (
        <ImagePreviewModal
          result={selected}
          downloaded={libraryFor(selected)}
          busy={busy}
          t={t}
          imageSrc={fileSrc(selected.thumbUrl)}
          onClose={() => setSelected(null)}
          onDownload={() => void download(selected)}
          onUse={() => void applyResult(selected)}
        />
      )}
    </section>
  );
}
