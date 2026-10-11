import { useEffect, useMemo, useRef, useState } from "react";
import {
  bumpSize,
  crowdBlock,
  defaultCrowd,
  sanitizeCrowd,
  type CrowdLang,
  type CrowdSettings,
} from "../lib/crowd";
import { getLang, setLang, useLang, useT } from "../lib/i18n";
import { KEYS, safeStorage } from "../lib/safeStorage";
import { MAX_PRAYER_CHARS, flushPrayers, submitPrayer } from "../net/prayer";
import { serverOrigin } from "../net/pairing";
import { useRemote } from "../state/RemoteContext";
import { Logo, OfflineBanner, StatusDot } from "./common";
import type { Slide } from "@hollyshow/protocol";

function prefersDark(): boolean {
  try {
    return globalThis.matchMedia?.("(prefers-color-scheme: light)").matches !== true;
  } catch {
    return true;
  }
}

function useCrowdSettings(): [CrowdSettings, (f: (s: CrowdSettings) => CrowdSettings) => void] {
  const [s, setS] = useState<CrowdSettings>(() =>
    sanitizeCrowd(safeStorage.getJson<unknown>(KEYS.crowd, null), defaultCrowd(prefersDark())),
  );
  const update = (f: (s: CrowdSettings) => CrowdSettings) =>
    setS((cur) => {
      const n = f(cur);
      safeStorage.setJson(KEYS.crowd, n);
      return n;
    });
  return [s, update];
}

/** Public view: follow the live slide (lyrics or Bible), choose text size, language, version, theme. */
export function Congregation() {
  const t = useT();
  const uiLang = useLang();
  const { session, status } = useRemote();
  const [s, update] = useCrowdSettings();
  const [prayerOpen, setPrayerOpen] = useState(false);
  const view = session.view;
  const slide: Slide | null = view?.live?.slide ?? null;

  // Previous slide, shown muted above the current one (as in the design).
  const cur = useRef<Slide | null>(null);
  const prev = useRef<Slide | null>(null);
  if (slide?.id !== cur.current?.id || slide?.lines.join("|") !== cur.current?.lines.join("|")) {
    if (cur.current && slide) prev.current = cur.current;
    cur.current = slide;
  }

  const block = useMemo(() => crowdBlock(slide, s), [slide, s]);
  const previous = useMemo(
    () => (prev.current && prev.current.id !== slide?.id ? crowdBlock(prev.current, s) : null),
    [slide, s],
  );
  const versions = block?.versions ?? [];
  const hidden = view ? view.blackout || view.clear : false;

  useEffect(() => {
    void flushPrayers(serverOrigin().http);
  }, []);

  return (
    <div className={`crowd ${s.dark ? "dark" : "light"}`}>
      <header className="crowd-head">
        <Logo />
        <div className="head-text">
          <span className="head-title">{session.serverName || "HollyShow"}</span>
          <span className="head-sub">
            <StatusDot status={status} /> {t("crowd.live")}
          </span>
        </div>
        <button
          className="crowd-btn"
          aria-label={t("crowd.smaller")}
          onClick={() => update((c) => bumpSize(c, -3))}
        >
          A−
        </button>
        <button
          className="crowd-btn strong"
          aria-label={t("crowd.bigger")}
          onClick={() => update((c) => bumpSize(c, 3))}
        >
          A+
        </button>
      </header>
      <OfflineBanner status={status} />

      <main className="crowd-body" aria-live="polite">
        {!view || (!slide && !view.logo) ? (
          <p className="crowd-muted">{t("crowd.waiting")}</p>
        ) : view.logo && !view.blackout ? (
          <div className="crowd-logo">
            <Logo size={72} />
          </div>
        ) : hidden || !block ? (
          <p className="crowd-muted">{view.blackout ? t("crowd.blackout") : ""}</p>
        ) : (
          <>
            {block.caption && <span className="crowd-caption">{block.caption}</span>}
            {previous && previous.text && (
              <p className="crowd-prev" style={{ fontSize: Math.round(s.size * 0.8) }}>
                {previous.text}
              </p>
            )}
            <p className="crowd-main" style={{ fontSize: s.size }}>
              {block.text}
            </p>
            {block.translation && (
              <p className="crowd-trans" lang="en" style={{ fontSize: Math.round(s.size * 0.65) }}>
                {block.translation}
              </p>
            )}
          </>
        )}
      </main>

      <footer className="crowd-foot">
        <div className="two">
          <label className="crowd-field">
            {t("crowd.lang")}
            <select
              value={s.lang}
              onChange={(e) => update((c) => ({ ...c, lang: e.target.value as CrowdLang }))}
            >
              <option value="both">{t("crowd.lang.both")}</option>
              <option value="es">{t("crowd.lang.es")}</option>
              <option value="en">{t("crowd.lang.en")}</option>
            </select>
          </label>
          <label className="crowd-field">
            {t("crowd.version")}
            <select
              className="mono"
              value={s.version ?? ""}
              onChange={(e) => update((c) => ({ ...c, version: e.target.value || null }))}
            >
              <option value="">{t("crowd.versionAuto")}</option>
              {[...new Set([...versions, ...(s.version ? [s.version] : [])])].map((v) => (
                <option key={v} value={v}>
                  {v}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="crowd-actions">
          <button className="crowd-wide" onClick={() => setPrayerOpen(true)}>
            {t("crowd.prayer")}
          </button>
          <button
            className="crowd-btn"
            aria-label={t("crowd.theme")}
            aria-pressed={s.dark}
            onClick={() => update((c) => ({ ...c, dark: !c.dark }))}
          >
            {s.dark ? "☀" : "☾"}
          </button>
          <button className="crowd-btn" onClick={() => setLang(getLang() === "es" ? "en" : "es")}>
            {uiLang === "es" ? "EN" : "ES"}
          </button>
        </div>
      </footer>

      {prayerOpen && <PrayerSheet onClose={() => setPrayerOpen(false)} />}
    </div>
  );
}

function PrayerSheet({ onClose }: { onClose: () => void }) {
  const t = useT();
  const [text, setText] = useState("");
  const [name, setName] = useState("");
  const [result, setResult] = useState<"sent" | "queued" | "empty" | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (!text.trim()) {
      setResult("empty");
      return;
    }
    setBusy(true);
    const r = await submitPrayer(serverOrigin().http, { text, name });
    setBusy(false);
    setResult(r);
    if (r === "sent" || r === "queued") setText("");
  };

  return (
    <div
      className="sheet-back"
      role="dialog"
      aria-modal="true"
      aria-label={t("crowd.prayer.title")}
    >
      <div className="sheet">
        <h2>{t("crowd.prayer.title")}</h2>
        <textarea
          className="crowd-input"
          rows={5}
          maxLength={MAX_PRAYER_CHARS}
          value={text}
          placeholder={t("crowd.prayer.text")}
          aria-label={t("crowd.prayer.text")}
          onChange={(e) => {
            setText(e.target.value);
            setResult(null);
          }}
        />
        <input
          className="crowd-input"
          maxLength={80}
          value={name}
          placeholder={t("crowd.prayer.name")}
          aria-label={t("crowd.prayer.name")}
          onChange={(e) => setName(e.target.value)}
        />
        {result && (
          <p role="status" className="crowd-muted">
            {result === "sent"
              ? t("crowd.prayer.sent")
              : result === "queued"
                ? t("crowd.prayer.queued")
                : t("crowd.prayer.empty")}
          </p>
        )}
        <div className="two">
          <button className="crowd-wide" onClick={onClose}>
            {result === "sent" || result === "queued"
              ? t("common.close")
              : t("crowd.prayer.cancel")}
          </button>
          <button className="crowd-wide strong" disabled={busy} onClick={() => void submit()}>
            {t("crowd.prayer.send")}
          </button>
        </div>
      </div>
    </div>
  );
}
