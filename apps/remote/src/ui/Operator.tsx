import { commands, type ShowEntry, type Timer } from "@hollyshow/protocol";
import { useState } from "react";
import { timerCmd } from "../lib/cmd";
import { useT } from "../lib/i18n";
import { safeStorage, KEYS } from "../lib/safeStorage";
import {
  findEntry,
  formatClock,
  itemClockMs,
  slideKey,
  slideText,
  timerValueMs,
} from "../lib/slideText";
import { useRemote, useServerNow } from "../state/RemoteContext";
import { BibleView } from "./Bible";
import {
  Header,
  IconBook,
  IconChat,
  IconLeft,
  IconList,
  IconRight,
  IconSlides,
  MiniPreview,
  OfflineBanner,
} from "./common";

type Tab = "slides" | "service" | "bible" | "messages";
const TABS: Array<{ id: Tab; icon: typeof IconSlides }> = [
  { id: "slides", icon: IconSlides },
  { id: "service", icon: IconList },
  { id: "bible", icon: IconBook },
  { id: "messages", icon: IconChat },
];

function initialTab(): Tab {
  const saved = safeStorage.get(KEYS.tab);
  return TABS.some((x) => x.id === saved) ? (saved as Tab) : "slides";
}

export function Operator() {
  const t = useT();
  const { session, status } = useRemote();
  const [tab, setTab] = useState<Tab>(initialTab);
  const view = session.view;
  const entry = view ? findEntry(session.show, view.live) : null;
  const choose = (x: Tab) => {
    setTab(x);
    safeStorage.set(KEYS.tab, x);
  };

  return (
    <div className="screen">
      <Header
        title={entry?.title ?? "HollyShow"}
        status={status}
        serverName={session.serverName}
        role={session.role}
      />
      <OfflineBanner status={status} />
      <div className="screen-body">
        {tab === "slides" && <SlidesTab />}
        {tab === "service" && <ServiceTab />}
        {tab === "bible" && <BibleView role="operator" />}
        {tab === "messages" && <MessagesTab />}
      </div>
      <nav className="tabbar" aria-label={t("tab.sections")}>
        {TABS.map(({ id, icon: Icon }) => (
          <button
            key={id}
            className={tab === id ? "on" : ""}
            aria-current={tab === id ? "page" : undefined}
            onClick={() => choose(id)}
          >
            <Icon />
            {t(`tab.${id}`)}
          </button>
        ))}
      </nav>
    </div>
  );
}

function SlidesTab() {
  const t = useT();
  const { session, send } = useRemote();
  const [toPreview, setToPreview] = useState(false);
  const view = session.view;
  if (!view) return null;
  const entry = findEntry(session.show, view.live);
  const live = view.live;
  const previewIdx =
    view.preview && live && view.preview.itemId === live.itemId ? view.preview.slideIndex : -1;
  const target = toPreview ? "preview" : "live";

  return (
    <>
      <div className="pad-x">
        <MiniPreview
          cue={live}
          kind="live"
          blackout={view.blackout}
          clear={view.clear}
          logo={view.logo}
        />
      </div>

      <div className="scroll slide-list">
        {session.show.length === 0 && <p className="muted">{t("op.noShow")}</p>}
        {entry?.slides.map((s, i) => {
          const cls =
            live && i === live.slideIndex
              ? "slide live"
              : i === previewIdx
                ? "slide next"
                : "slide";
          return (
            <button
              key={s.id}
              className={cls}
              aria-current={cls === "slide live" ? "true" : undefined}
              onClick={() => void send(commands.goTo(entry.itemId, i, target))}
            >
              <span className="slide-key mono">{slideKey(s.label)}</span>
              <span className="slide-text">{slideText(s)}</span>
            </button>
          );
        })}
        {!entry && session.show.length > 0 && (
          <div className="slide-list-hint">
            {session.show.map((e) => (
              <ItemButton key={e.itemId} e={e} target={target} />
            ))}
          </div>
        )}
      </div>

      <div className="seg" role="group" aria-label="Destino">
        <button className={!toPreview ? "on" : ""} onClick={() => setToPreview(false)}>
          {t("op.goLive")}
        </button>
        <button className={toPreview ? "on" : ""} onClick={() => setToPreview(true)}>
          {t("op.goPreview")}
        </button>
      </div>

      <div className="triple">
        <button
          className={`ctl black${view.blackout ? " on" : ""}`}
          aria-pressed={view.blackout}
          onClick={() => void send(commands.blackout(!view.blackout))}
        >
          {t("op.black")}
        </button>
        <button
          className={`ctl${view.clear ? " on" : ""}`}
          aria-pressed={view.clear}
          onClick={() => void send(commands.clear(!view.clear))}
        >
          {t("op.clear")}
        </button>
        <button
          className={`ctl${view.logo ? " on" : ""}`}
          aria-pressed={view.logo}
          onClick={() => void send(commands.logo(!view.logo))}
        >
          {t("op.logo")}
        </button>
      </div>

      <div className="pager">
        <button
          className="big"
          aria-label={t("op.prev")}
          onClick={() => void send(commands.prev(view.version))}
        >
          <IconLeft size={34} />
        </button>
        <button
          className="big primary"
          aria-label={t("op.next")}
          onClick={() => void send(commands.next(view.version))}
        >
          <IconRight size={34} />
        </button>
      </div>
    </>
  );
}

function ItemButton({ e, target }: { e: ShowEntry; target: "live" | "preview" }) {
  const { send } = useRemote();
  return (
    <button className="slide" onClick={() => void send(commands.showItem(e.itemId, target))}>
      <span className="slide-text">{e.title}</span>
    </button>
  );
}

function ServiceTab() {
  const t = useT();
  const { client, session, send } = useRemote();
  const now = useServerNow(client);
  const view = session.view;
  if (!view) return null;
  const clock = itemClockMs(view, now);

  return (
    <div className="scroll">
      {view.serviceMode && <div className="chip-note">🔒 {t("op.serviceMode")}</div>}
      {clock !== null && (
        <div className="card row">
          <span className="muted">{t("op.itemClock")}</span>
          <span className={`mono big-num${clock < 0 ? " over" : ""}`}>{formatClock(clock)}</span>
        </div>
      )}
      <ul className="list" aria-label={t("tab.service")}>
        {session.show.map((e, i) => {
          const isLive = view.live?.itemId === e.itemId;
          return (
            <li key={e.itemId}>
              <button
                className={`row-btn${isLive ? " live" : ""}`}
                aria-current={isLive ? "true" : undefined}
                onClick={() => void send(commands.showItem(e.itemId, "live"))}
              >
                <span className="mono idx">{i + 1}</span>
                <span className="grow">
                  <span className="title">{e.title}</span>
                  <span className="muted small">
                    {e.slides.length} slides
                    {e.durationSecs ? ` · ${formatClock(e.durationSecs * 1000)}` : ""}
                  </span>
                </span>
              </button>
            </li>
          );
        })}
      </ul>
      {view.timers.length > 0 && <h3 className="section-title">{t("op.timers")}</h3>}
      {view.timers.map((tm) => (
        <TimerRow key={tm.id} timer={tm} now={now} />
      ))}
    </div>
  );
}

function TimerRow({ timer, now }: { timer: Timer; now: number }) {
  const t = useT();
  const { send } = useRemote();
  const ms = timerValueMs(timer, now);
  const running = timer.run === "running";
  return (
    <div className="card row">
      <div className="grow">
        <div className="title">{timer.label}</div>
        <span className={`mono big-num${ms < 0 ? " over" : ""}`}>{formatClock(ms)}</span>
        {ms < 0 && <span className="muted small"> {t("op.overtime")}</span>}
      </div>
      <button
        className="ctl small-btn"
        onClick={() => void send(running ? timerCmd.pause(timer.id) : timerCmd.start(timer.id))}
      >
        {running ? t("op.pause") : t("op.start")}
      </button>
      <button className="ctl small-btn" onClick={() => void send(timerCmd.reset(timer.id))}>
        {t("op.reset")}
      </button>
    </div>
  );
}

function MessagesTab() {
  const t = useT();
  const { session, send } = useRemote();
  const [text, setText] = useState("");
  const current = session.view?.stageMessage?.text ?? null;
  const quick = [t("msg.quick1"), t("msg.quick2"), t("msg.quick3"), t("msg.quick4")];
  const submit = () => {
    const v = text.trim();
    if (!v) return;
    void send(commands.stageMessage(v)).then((a) => {
      if (a?.ok) setText("");
    });
  };
  return (
    <div className="scroll">
      <h3 className="section-title">{t("msg.title")}</h3>
      {current && (
        <div className="card row">
          <div className="grow">
            <span className="muted small">{t("msg.current")}</span>
            <div>{current}</div>
          </div>
          <button className="ctl small-btn" onClick={() => void send(commands.stageMessage(null))}>
            {t("msg.remove")}
          </button>
        </div>
      )}
      <textarea
        className="input"
        rows={3}
        maxLength={280}
        value={text}
        placeholder={t("msg.placeholder")}
        aria-label={t("msg.title")}
        onChange={(e) => setText(e.target.value)}
      />
      <div className="chips">
        {quick.map((q) => (
          <button key={q} className="chip" onClick={() => void send(commands.stageMessage(q))}>
            {q}
          </button>
        ))}
      </div>
      <button className="cta" disabled={!text.trim()} onClick={submit}>
        {t("msg.send")}
      </button>
    </div>
  );
}
