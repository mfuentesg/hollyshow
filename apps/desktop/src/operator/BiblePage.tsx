import type { ParallelLayout, Passage, Verse } from "@hollyshow/protocol";
import { SlideView } from "@hollyshow/slide-renderer";
import { Button, Chip, SwitchRow } from "@hollyshow/ui";
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useApi } from "../api/context";
import { usePref } from "../hooks/usePref";
import { bookByOsis, bookName, formatPassage, parseReference, shortPassage } from "../lib/bibleRef";
import { composeBibleSlide, type VersionText } from "../lib/bibleSlide";
import { BOOKS } from "../lib/books";
import { cmd } from "../lib/cmd";
import { applyMarks } from "../lib/marks";
import { addItem, newItem } from "../lib/playlistOps";
import { useOperator } from "./OperatorContext";
import { useBibleSelection } from "./useBibleSelection";

const LAYOUTS: ParallelLayout[] = ["columns", "stacked", "subtitle"];

function LayoutIcon({ layout }: { layout: ParallelLayout }) {
  const d =
    layout === "columns"
      ? "M20 4v16M5 8h11M5 12h11M24 8h11M24 12h11"
      : layout === "stacked"
        ? "M4 12h32M8 6h24M8 17h24"
        : "M6 8h28M6 12h28M12 19h16";
  return (
    <svg
      width="40"
      height="24"
      viewBox="0 0 40 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      aria-hidden="true"
    >
      <rect x="1" y="1" width="38" height="22" rx="3" />
      <path d={d} />
    </svg>
  );
}

interface HistoryEntry {
  ref: string;
  at: number;
}

/** Bible screen (docs/design/Bible.dc.html): reference, versions, chapters, parallel table, layout. */
export function BiblePage() {
  const { t, i18n } = useTranslation();
  const lang = i18n.language.startsWith("en") ? "en" : "es";
  const api = useApi();
  const nav = useNavigate();
  const op = useOperator();
  const { versions, playlist } = op;
  const [params, setParams] = useSearchParams();
  const [selection, setSelection] = useBibleSelection(versions);
  const [refText, setRefText] = useState(params.get("ref") ?? "Jn 3:16-18");
  const [layout, setLayout] = usePref<ParallelLayout>("bible.layout", "columns");
  const [opts, setOpts] = usePref("bible.options", {
    versePerSlide: true,
    wordsOfJesusRed: false,
    showCopyright: true,
  });
  const [history, setHistory] = usePref<HistoryEntry[]>("bible.history", []);
  const [chapterVerses, setChapterVerses] = useState<Record<string, Verse[]>>({});
  const [passageVerses, setPassageVerses] = useState<Record<string, Verse[]>>({});
  const [page, setPage] = useState(0);
  const anchor = useRef<number | null>(null);
  const rowsRef = useRef<HTMLTableSectionElement>(null);

  // Deep link from the command palette (?ref=...).
  const refParam = params.get("ref");
  useEffect(() => {
    if (refParam) {
      setRefText(refParam);
      setParams({}, { replace: true });
    }
  }, [refParam, setParams]);

  const parsed = useMemo(() => parseReference(refText, lang), [refText, lang]);
  const book = parsed?.book ?? bookByOsis("John")!;
  const chapter = parsed?.passage.start.chapter ?? 1;
  const passage: Passage | null = parsed?.passage ?? null;
  const shown = selection
    .map((c) => versions.find((v) => v.code === c))
    .filter((v): v is NonNullable<typeof v> => !!v);

  const chapterKey = `${book.osis}.${chapter}.${selection.join(",")}`;
  useEffect(() => {
    let alive = true;
    const whole: Passage = {
      start: { book: book.osis, chapter, verse: null },
      end: { book: book.osis, chapter, verse: null },
    };
    void Promise.all(
      selection.map((code) =>
        api
          .query({ kind: "getPassage", version: code, passage: whole })
          .then((r) => [code, r.verses] as const),
      ),
    ).then((rs) => {
      if (alive) setChapterVerses(Object.fromEntries(rs));
    });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api, chapterKey]);

  const passageKey = passage ? `${JSON.stringify(passage)}|${selection.join(",")}` : "";
  useEffect(() => {
    if (!passage) {
      setPassageVerses({});
      return;
    }
    let alive = true;
    void Promise.all(
      selection.map((code) =>
        api
          .query({ kind: "getPassage", version: code, passage })
          .then((r) => [code, r.verses] as const),
      ),
    ).then((rs) => {
      if (alive) setPassageVerses(Object.fromEntries(rs));
    });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api, passageKey]);

  useEffect(() => setPage(0), [passageKey, opts.versePerSlide]);

  // Verse numbers of the chapter (union of versions) and the selected range.
  const verseNumbers = useMemo(() => {
    const set = new Set<number>();
    for (const vs of Object.values(chapterVerses))
      for (const v of vs)
        if (v.reference.verse && v.reference.chapter === chapter) set.add(v.reference.verse);
    return [...set].sort((a, b) => a - b);
  }, [chapterVerses, chapter]);

  const range = useMemo(() => {
    if (!passage) return null;
    const s = passage.start;
    const e = passage.end;
    if (s.chapter !== chapter || e.chapter !== chapter) return { lo: 0, hi: 0, multi: true };
    return { lo: s.verse ?? 1, hi: e.verse ?? 999, multi: false };
  }, [passage, chapter]);

  const selectedVerses = useMemo(() => {
    const nums = new Set<number>();
    for (const vs of Object.values(passageVerses))
      for (const v of vs) if (v.reference.verse) nums.add(v.reference.verse);
    return [...nums].sort((a, b) => a - b);
  }, [passageVerses]);

  const pages = useMemo<Passage[]>(() => {
    if (!passage) return [];
    if (!opts.versePerSlide || selectedVerses.length <= 1) return [passage];
    const first = Object.values(passageVerses)[0]?.[0]?.reference;
    return selectedVerses.map((n) => {
      const ch =
        Object.values(passageVerses)
          .flat()
          .find((v) => v.reference.verse === n)?.reference.chapter ??
        first?.chapter ??
        chapter;
      const r = { book: book.osis, chapter: ch, verse: n };
      return { start: r, end: r };
    });
  }, [passage, opts.versePerSlide, selectedVerses, passageVerses, book.osis, chapter]);

  const current = pages[Math.min(page, Math.max(0, pages.length - 1))] ?? null;
  const previewSlide = useMemo(() => {
    if (!current) return null;
    const texts: VersionText[] = shown.map((v) => ({
      code: v.code,
      language: v.language,
      copyright: v.copyrightText,
      verses: (passageVerses[v.code] ?? []).filter((vv) => inPage(vv, current)),
    }));
    return composeBibleSlide("bible:preview", current, texts, layout, {
      showCopyright: opts.showCopyright,
      showVersionLabel: true,
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current, passageVerses, layout, opts.showCopyright, shown.map((v) => v.code).join(",")]);

  const remember = (text: string) =>
    setHistory(
      [{ ref: text, at: Date.now() }, ...history.filter((h) => h.ref !== text)].slice(0, 8),
    );

  const setPassageText = (pa: Passage) => setRefText(shortPassage(pa, lang));

  const clickVerse = (n: number, shift: boolean) => {
    const a = shift && anchor.current !== null ? anchor.current : n;
    if (!shift) anchor.current = n;
    const lo = Math.min(a, n);
    const hi = Math.max(a, n);
    setPassageText({
      start: { book: book.osis, chapter, verse: lo },
      end: { book: book.osis, chapter, verse: hi },
    });
  };

  const sendPreview = async (target: "preview" | "live", pg = current) => {
    if (!pg || selection.length === 0) return;
    const ack = await op.send(cmd.showBible(pg, selection, target, layout));
    if (ack?.ok && passage) remember(shortPassage(passage, lang));
  };

  const addToService = () => {
    if (!passage || !playlist) return;
    const label = formatPassage(passage, lang);
    const item = newItem(
      t("service.reading", { ref: label }),
      { kind: "bible", passage, versions: selection, layout },
      120,
    );
    op.editPlaylist((pl) => addItem(pl, item));
    remember(shortPassage(passage, lang));
    op.selectItem(item.id);
  };

  const toggleVersion = (code: string) => {
    if (selection.includes(code)) {
      if (selection.length > 1) setSelection(selection.filter((c) => c !== code));
    } else if (selection.length < 4) setSelection([...selection, code]);
  };

  const onRowKey = (e: React.KeyboardEvent<HTMLTableRowElement>, n: number, idx: number) => {
    const rows = rowsRef.current?.querySelectorAll<HTMLElement>("tr[data-verse]");
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = rows?.[idx + (e.key === "ArrowDown" ? 1 : -1)];
      if (next) {
        next.focus();
        if (e.shiftKey) clickVerse(Number(next.dataset["verse"]), true);
      }
    } else if (e.key === " ") {
      e.preventDefault();
      clickVerse(n, e.shiftKey);
    } else if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      void sendPreview("preview");
    }
  };

  const currentBookName = bookName(book.osis, lang);
  const total = pages.length;

  return (
    <div className="bible-page">
      <aside className="panel panel--left" aria-label={t("bible.reference")}>
        <div className="field-block">
          <label className="muted" htmlFor="bible-ref">
            {t("bible.reference")}
          </label>
          <div className={`refbox${refText && !parsed ? " is-invalid" : ""}`}>
            <input
              id="bible-ref"
              className="refbox__input"
              aria-label={t("bible.referenceLabel")}
              aria-describedby="bible-ref-resolved"
              aria-invalid={refText !== "" && !parsed}
              value={refText}
              onChange={(e) => setRefText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  void sendPreview(e.ctrlKey || e.metaKey ? "live" : "preview");
                }
              }}
              spellCheck={false}
              autoComplete="off"
            />
          </div>
          <span id="bible-ref-resolved" className="muted small" aria-live="polite">
            {parsed ? (
              <>
                → <b className="strong">{formatPassage(parsed.passage, lang)}</b>
                {selectedVerses.length > 0 &&
                  ` · ${t("bible.verseCount", { count: selectedVerses.length })}`}
              </>
            ) : refText ? (
              t("bible.invalidReference")
            ) : (
              t("bible.typeReference")
            )}
          </span>
        </div>

        <div className="field-block">
          <span className="muted">{t("bible.versionsOnScreen")}</span>
          <div className="row gap-8 wrap" role="group" aria-label={t("bible.versionsOnScreen")}>
            {versions.map((v) => {
              const idx = selection.indexOf(v.code);
              return (
                <Chip
                  key={v.code}
                  pressed={idx >= 0}
                  onClick={() => toggleVersion(v.code)}
                  title={v.name}
                  disabled={idx < 0 && selection.length >= 4}
                >
                  {idx >= 0 ? `${idx + 1} · ` : ""}
                  {v.code}
                </Chip>
              );
            })}
            <Chip tone="dashed" onClick={() => nav("/operator/settings/bible")}>
              {t("bible.import")}
            </Chip>
          </div>
        </div>

        <div className="field-block">
          <label className="muted" htmlFor="bible-book">
            {t("bible.book")}
          </label>
          <select
            id="bible-book"
            className="hs-select hs-select--bg"
            value={book.osis}
            onChange={(e) =>
              setRefText(
                `${BOOKS.find((b) => b.osis === e.target.value)?.[lang][1] ?? e.target.value} 1`,
              )
            }
          >
            {BOOKS.map((b) => (
              <option key={b.osis} value={b.osis}>
                {b[lang][0]}
              </option>
            ))}
          </select>
          <span className="muted">{t("bible.chapters", { book: currentBookName })}</span>
          <div
            className="chapters"
            role="group"
            aria-label={t("bible.chapters", { book: currentBookName })}
          >
            {Array.from({ length: book.chapters }, (_, i) => i + 1).map((n) => (
              <button
                key={n}
                type="button"
                className={`chapter${n === chapter ? " is-on" : ""}`}
                aria-pressed={n === chapter}
                aria-label={t("bible.chapterN", { n })}
                onClick={() => setRefText(`${book[lang][1]} ${n}`)}
              >
                {n}
              </button>
            ))}
          </div>
        </div>

        <div className="field-block">
          <span className="muted">{t("bible.history")}</span>
          {history.length === 0 && <span className="muted small">{t("bible.noHistory")}</span>}
          {history.map((h) => (
            <button
              key={h.ref}
              type="button"
              className="history-row"
              onClick={() => setRefText(h.ref)}
            >
              <span className="mono">{h.ref}</span>
              <span className="muted">
                {new Date(h.at).toLocaleTimeString(undefined, {
                  hour: "2-digit",
                  minute: "2-digit",
                  hour12: false,
                })}
              </span>
            </button>
          ))}
        </div>
      </aside>

      <main className="center" aria-label={t("bible.passageTable")}>
        <div className="center__head">
          <h1>
            {currentBookName} {chapter}
          </h1>
          <span className="muted small">{t("bible.tableHelp")}</span>
        </div>
        <div className="table-wrap">
          <table className="verse-table" aria-label={t("bible.passageTable")}>
            <thead>
              <tr>
                <th scope="col" className="vt-num">
                  <span className="hs-sr-only">{t("bible.verse")}</span>
                </th>
                {shown.map((v) => (
                  <th key={v.code} scope="col" className="vt-head mono">
                    {v.code} · {v.name}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody ref={rowsRef}>
              {verseNumbers.map((n, idx) => {
                const sel = range ? !range.multi && n >= range.lo && n <= range.hi : false;
                return (
                  <tr
                    key={n}
                    data-verse={n}
                    className={sel ? "is-sel" : undefined}
                    tabIndex={0}
                    aria-selected={sel}
                    onClick={(e) => clickVerse(n, e.shiftKey)}
                    onKeyDown={(e) => onRowKey(e, n, idx)}
                  >
                    <th scope="row" className="vt-num mono">
                      {n}
                    </th>
                    {shown.map((v) => {
                      const verse = chapterVerses[v.code]?.find(
                        (x) => x.reference.verse === n && x.reference.chapter === chapter,
                      );
                      return (
                        <td key={v.code} className="vt-cell">
                          {verse ? (
                            applyMarks(verse.text, verse.marks).map((s, i) => (
                              <Fragment key={i}>
                                <span
                                  className={
                                    [
                                      opts.wordsOfJesusRed && s.jesus ? "jesus" : "",
                                      s.italic ? "italic" : "",
                                    ]
                                      .join(" ")
                                      .trim() || undefined
                                  }
                                >
                                  {s.text}
                                </span>
                              </Fragment>
                            ))
                          ) : (
                            <em className="muted">{t("bible.missingVerse")}</em>
                          )}
                        </td>
                      );
                    })}
                  </tr>
                );
              })}
              {verseNumbers.length === 0 && (
                <tr>
                  <td className="muted pad" colSpan={shown.length + 1}>
                    {t("bible.noText")}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        <p className="muted small">{t("bible.versificationNote")}</p>
      </main>

      <aside className="panel panel--right" aria-label={t("operator.preview")}>
        <section className="monitor monitor--preview">
          <div className="monitor__head">
            <span className="monitor__preview-tag">
              {t("operator.preview").toUpperCase()}
              {total > 1
                ? ` · ${t("bible.pageOf", { n: Math.min(page, total - 1) + 1, total })}`
                : ""}
            </span>
            <span className="muted small">{current ? shortPassage(current, lang) : ""}</span>
          </div>
          <div className="monitor__screen" data-testid="bible-preview">
            <SlideView slide={previewSlide} theme={op.themeFor(null)} />
          </div>
          {total > 1 && (
            <div className="row gap-8">
              <Button
                size="sm"
                style={{ flex: 1 }}
                disabled={page === 0}
                onClick={() => {
                  setPage(page - 1);
                  void sendPreview("preview", pages[page - 1]);
                }}
              >
                ◀ {t("bible.prevVerse")}
              </Button>
              <Button
                size="sm"
                style={{ flex: 1 }}
                disabled={page >= total - 1}
                onClick={() => {
                  setPage(page + 1);
                  void sendPreview("preview", pages[page + 1]);
                }}
              >
                {t("bible.nextVerse")} ▶
              </Button>
            </div>
          )}
        </section>

        <div className="field-block" role="radiogroup" aria-label={t("bible.layout")}>
          <span className="muted">{t("bible.layout")}</span>
          <div className="layouts">
            {LAYOUTS.map((l) => (
              <button
                key={l}
                type="button"
                role="radio"
                aria-checked={layout === l}
                className={`layout-btn${layout === l ? " is-on" : ""}`}
                onClick={() => setLayout(l)}
              >
                <LayoutIcon layout={l} />
                {t(`bible.layouts.${l}`)}
              </button>
            ))}
          </div>
        </div>

        <div className="field-block">
          <span className="muted">{t("bible.options")}</span>
          <SwitchRow
            label={t("bible.versePerSlide")}
            checked={opts.versePerSlide}
            onChange={(v) => setOpts({ ...opts, versePerSlide: v })}
          />
          <SwitchRow
            label={t("bible.wordsOfJesus")}
            checked={opts.wordsOfJesusRed}
            onChange={(v) => setOpts({ ...opts, wordsOfJesusRed: v })}
          />
          <SwitchRow
            label={t("bible.showCopyright")}
            checked={opts.showCopyright}
            onChange={(v) => setOpts({ ...opts, showCopyright: v })}
          />
          <span className="muted small">{t("bible.optionsNote")}</span>
        </div>

        <div className="row gap-8 push-bottom">
          <Button
            style={{ flex: 1, minHeight: 48 }}
            disabled={!passage || !playlist}
            onClick={addToService}
          >
            {t("bible.addToService")}
          </Button>
          <Button
            variant="primary"
            style={{ flex: 1, minHeight: 48 }}
            disabled={!current || selection.length === 0}
            onClick={() => void sendPreview("live")}
          >
            {t("bible.showLive")}
          </Button>
        </div>
      </aside>
    </div>
  );
}

function inPage(v: Verse, pg: Passage): boolean {
  const o = v.reference.chapter * 1000 + (v.reference.verse ?? 0);
  const lo = pg.start.chapter * 1000 + (pg.start.verse ?? 0);
  const hi = pg.end.chapter * 1000 + (pg.end.verse ?? 999);
  return v.reference.book === pg.start.book && o >= lo && o <= hi;
}
