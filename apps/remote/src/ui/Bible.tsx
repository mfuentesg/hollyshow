import {
  commands,
  type BibleHit,
  type BibleVersion,
  type ParallelLayout,
  type Verse,
  type VersionCode,
} from "@hollyshow/protocol";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  bookInfo,
  chapterPassage,
  parseDisplayedReference,
  parseReference,
  stepChapter,
  versePassage,
} from "../lib/bibleRef";
import { useT } from "../lib/i18n";
import { KEYS, safeStorage } from "../lib/safeStorage";
import { useRemote } from "../state/RemoteContext";
import { IconLeft, IconRight, IconSearch } from "./common";

interface Chapter {
  book: string;
  chapter: number;
  verses: Verse[];
}

function bookTitle(book: string, lang: string): string {
  const info = bookInfo(book);
  if (!info) return book;
  return (lang.startsWith("en") ? info.en[0] : info.es[0]) ?? book;
}

/** Reader range currently on the projector, parsed from the live Bible slide's reference. */
export function liveRange(
  slide: { kind: string; bible: Array<{ reference: string }> } | undefined,
): { book: string; chapter: number; from: number; to: number } | null {
  if (!slide || slide.kind !== "bible") return null;
  const ref = slide.bible[0]?.reference;
  if (!ref) return null;
  const p = parseDisplayedReference(ref);
  if (!p) return null;
  const from = p.verse ?? 1;
  return {
    book: p.book,
    chapter: p.chapter,
    from,
    to: p.verseEnd ?? (p.verse === null ? 999 : from),
  };
}

export function BibleView({ role }: { role: "pastor" | "operator" }) {
  const t = useT();
  const { session, send, client, toast } = useRemote();
  const [versions, setVersions] = useState<BibleVersion[] | null>(null);
  const [chosen, setChosen] = useState<VersionCode[]>(() =>
    safeStorage.getJson<VersionCode[]>(KEYS.bibleVersions, []),
  );
  const [input, setInput] = useState("");
  const [chapter, setChapter] = useState<Chapter | null>(null);
  const [hits, setHits] = useState<BibleHit[] | null>(null);
  const [sel, setSel] = useState<{ from: number; to: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [layout, setLayout] = useState<ParallelLayout>("columns");
  const reqId = useRef(0);
  const lang = typeof document === "undefined" ? "es" : document.documentElement.lang || "es";

  // Installed versions (re-read when the host says the Bible library changed).
  const loadVersions = useCallback(() => {
    client
      .query({ kind: "listBibleVersions" })
      .then((r) => {
        const items = r.items.filter((v) => v.isEnabled).sort((a, b) => a.sortOrder - b.sortOrder);
        setVersions(items);
        setChosen((cur) => {
          const valid = cur.filter((c) => items.some((v) => v.code === c));
          if (valid.length > 0) return valid;
          const def = items.find((v) => v.isDefault) ?? items[0];
          return def ? [def.code] : [];
        });
      })
      .catch(() => setVersions((v) => v ?? []));
  }, [client]);
  useEffect(() => {
    if (session.phase === "ready") loadVersions();
  }, [session.phase, loadVersions]);
  useEffect(
    () =>
      client.subscribe((u) => {
        if (u.events.some((e) => e.kind === "libraryChanged" && e.scope === "bible"))
          loadVersions();
      }),
    [client, loadVersions],
  );
  useEffect(() => {
    if (chosen.length > 0) safeStorage.setJson(KEYS.bibleVersions, chosen);
  }, [chosen]);

  const primary = chosen[0] ?? null;

  const loadChapter = useCallback(
    async (book: string, ch: number, pick?: { from: number; to: number } | null) => {
      if (!primary) return;
      const id = ++reqId.current;
      setBusy(true);
      try {
        const r = await client.query({
          kind: "getPassage",
          version: primary,
          passage: chapterPassage(book, ch),
        });
        if (id !== reqId.current) return;
        setChapter({ book, chapter: ch, verses: r.verses });
        setHits(null);
        setSel(pick ?? null);
      } catch {
        if (id === reqId.current) toast(t("err.offline"));
      } finally {
        if (id === reqId.current) setBusy(false);
      }
    },
    [client, primary, toast, t],
  );

  // Typed input: a reference opens the chapter, anything else is a text search.
  useEffect(() => {
    const text = input.trim();
    if (!text || !primary) {
      setHits(null);
      return;
    }
    const ref = parseReference(text);
    const handle = setTimeout(
      () => {
        if (ref) {
          const pick =
            ref.verse === null ? null : { from: ref.verse, to: ref.verseEnd ?? ref.verse };
          void loadChapter(ref.book, ref.chapter, pick);
          return;
        }
        if (text.length < 3) return;
        const id = ++reqId.current;
        setBusy(true);
        client
          .query({ kind: "searchBible", text, version: primary, limit: 30 })
          .then((r) => id === reqId.current && setHits(r.items))
          .catch(() => id === reqId.current && toast(t("err.offline")))
          .finally(() => id === reqId.current && setBusy(false));
      },
      ref ? 150 : 350,
    );
    return () => clearTimeout(handle);
  }, [input, primary, client, loadChapter, toast, t]);

  // Follow the projector when nothing is open yet.
  const live = session.view?.live?.slide;
  const lr = useMemo(() => liveRange(live), [live]);
  useEffect(() => {
    if (lr && !chapter && primary && !input) void loadChapter(lr.book, lr.chapter, null);
  }, [lr, chapter, primary, input, loadChapter]);

  const toggleVersion = (code: VersionCode) =>
    setChosen((cur) => {
      if (cur.includes(code)) return cur.length > 1 ? cur.filter((c) => c !== code) : cur;
      return cur.length >= 4 ? cur : [...cur, code];
    });

  // The reader belongs to the primary version: reload when it changes.
  const prevPrimary = useRef(primary);
  useEffect(() => {
    if (prevPrimary.current !== primary && chapter)
      void loadChapter(chapter.book, chapter.chapter, sel);
    prevPrimary.current = primary;
  }, [primary, chapter, sel, loadChapter]);

  const tapVerse = (n: number) =>
    setSel((cur) => {
      if (!cur || cur.from !== cur.to) return { from: n, to: n };
      return n === cur.from ? null : { from: Math.min(cur.from, n), to: Math.max(cur.from, n) };
    });

  const go = async (target: "live" | "preview") => {
    if (!chapter || !sel || chosen.length === 0) return;
    const ack = await send(
      commands.showBible(
        versePassage(chapter.book, chapter.chapter, sel.from, sel.to),
        chosen,
        target,
        layout,
      ),
    );
    if (ack?.ok) toast(t("bible.sent"));
  };

  const step = (d: 1 | -1) => {
    if (!chapter) return;
    const n = stepChapter(chapter.book, chapter.chapter, d);
    if (n) void loadChapter(n.book, n.chapter, null);
  };

  const onLive = (n: number) =>
    !!lr &&
    !!chapter &&
    lr.book === chapter.book &&
    lr.chapter === chapter.chapter &&
    n >= lr.from &&
    n <= lr.to;

  return (
    <div className="bible">
      <div className="bible-top">
        <label className="search">
          <IconSearch size={18} />
          <input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={t("bible.search")}
            aria-label={t("bible.search")}
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            enterKeyHint="search"
          />
        </label>
        <div className="chips" role="group" aria-label={t("bible.title")}>
          {versions && versions.length === 0 && (
            <span className="muted small">{t("bible.noVersions")}</span>
          )}
          {versions?.map((v) => (
            <button
              key={v.code}
              className={`chip mono${chosen.includes(v.code) ? " on" : ""}`}
              aria-pressed={chosen.includes(v.code)}
              title={v.name}
              onClick={() => toggleVersion(v.code)}
            >
              {v.code}
            </button>
          ))}
        </div>
      </div>

      <div className="scroll reader" aria-busy={busy}>
        {hits && (
          <ul className="list">
            {hits.length === 0 && <li className="muted">{t("bible.noResults")}</li>}
            {hits.map((h, i) => (
              <li key={`${h.reference.book}.${h.reference.chapter}.${h.reference.verse}.${i}`}>
                <button
                  className="row-btn"
                  onClick={() => {
                    const v = h.reference.verse;
                    void loadChapter(
                      h.reference.book,
                      h.reference.chapter,
                      v ? { from: v, to: v } : null,
                    );
                    setInput("");
                  }}
                >
                  <span className="grow">
                    <span className="title mono small">
                      {bookTitle(h.reference.book, lang)} {h.reference.chapter}:{h.reference.verse}
                    </span>
                    <span className="hit-text">{h.text}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}

        {!hits && chapter && (
          <>
            <div className="reader-head">
              <button className="icon-btn" aria-label={t("bible.prev")} onClick={() => step(-1)}>
                <IconLeft size={18} />
              </button>
              <span className="muted grow">
                {bookTitle(chapter.book, lang)} {chapter.chapter} · {t("bible.reader")}
              </span>
              <button className="icon-btn" aria-label={t("bible.next")} onClick={() => step(1)}>
                <IconRight size={18} />
              </button>
            </div>
            {chapter.verses.map((v) => {
              const n = v.reference.verse ?? 0;
              const picked = !!sel && n >= sel.from && n <= sel.to;
              const isLive = onLive(n);
              return (
                <p
                  key={n}
                  className={`verse${picked ? " picked" : ""}${isLive ? " onair" : ""}`}
                  aria-current={isLive ? "true" : undefined}
                  onClick={() => tapVerse(n)}
                >
                  <b className="mono">{n} </b>
                  {v.text}
                  {isLive && <span className="air-tag">{t("bible.onScreen")}</span>}
                </p>
              );
            })}
            <p className="muted small hint">{t("bible.select")}</p>
          </>
        )}

        {!hits && !chapter && <p className="muted hint">{t("bible.searchHint")}</p>}
      </div>

      <div className="bible-actions">
        {chosen.length > 1 && (
          <div className="seg full" role="group" aria-label={t("bible.layout")}>
            {(["columns", "stacked", "subtitle"] as const).map((l) => (
              <button key={l} className={layout === l ? "on" : ""} onClick={() => setLayout(l)}>
                {t(`bible.layout.${l}`)}
              </button>
            ))}
          </div>
        )}
        <div className="two">
          <button className="ctl tall" disabled={!sel} onClick={() => void go("preview")}>
            {t("bible.toPreview")}
          </button>
          <button className="cta tall" disabled={!sel} onClick={() => void go("live")}>
            {role === "pastor" ? t("bible.toLive") : t("bible.toLive.operator")}
          </button>
        </div>
        {role === "pastor" && <span className="muted small center">{t("bible.note")}</span>}
      </div>
    </div>
  );
}
