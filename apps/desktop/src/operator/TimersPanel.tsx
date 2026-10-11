import type { Timer, TimerKind } from "@hollyshow/protocol";
import { Button, IconButton, IconClose, Select, TextInput } from "@hollyshow/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNow } from "../hooks/useNow";
import { cmd } from "../lib/cmd";
import { uid } from "../lib/text";
import { formatDuration } from "../lib/schedule";
import { timerDisplaySecs, timerOvertime } from "../lib/timers";
import { useOperator } from "./OperatorContext";

/** Service timers: countdowns and stopwatches driven by the host clock. */
export function TimersPanel() {
  const { t } = useTranslation();
  const { p, send } = useOperator();
  const now = useNow(250);
  const [label, setLabel] = useState("");
  const [minutes, setMinutes] = useState("5");
  const [kind, setKind] = useState<TimerKind>("countdown");

  const add = () => {
    const timer: Timer = {
      id: uid("timer"),
      label: label.trim() || (kind === "countdown" ? t("timers.countdown") : t("timers.stopwatch")),
      kind,
      durationSecs: kind === "countdown" ? Math.max(1, Number(minutes) || 1) * 60 : 0,
      run: "stopped",
      startedAtMs: null,
      elapsedMs: 0,
    };
    void send(cmd.setTimer(timer));
    setLabel("");
  };

  return (
    <section className="card" aria-label={t("timers.title")}>
      <h3 className="card__title">{t("timers.title")}</h3>
      <ul className="timers">
        {p.view.timers.map((tm) => {
          const secs = timerDisplaySecs(tm, now);
          const over = timerOvertime(tm, now);
          return (
            <li key={tm.id} className="timer">
              <span className="timer__label">{tm.label}</span>
              <output
                className={`mono timer__time${over ? " overtime" : ""}`}
                aria-label={t("timers.remaining", { label: tm.label })}
              >
                {formatDuration(secs)}
              </output>
              {tm.run === "running" ? (
                <Button
                  size="sm"
                  onClick={() => void send(cmd.pauseTimer(tm.id))}
                  aria-label={t("timers.pause", { label: tm.label })}
                >
                  {t("timers.pauseShort")}
                </Button>
              ) : (
                <Button
                  size="sm"
                  onClick={() => void send(cmd.startTimer(tm.id))}
                  aria-label={t("timers.start", { label: tm.label })}
                >
                  {t("timers.startShort")}
                </Button>
              )}
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void send(cmd.resetTimer(tm.id))}
                aria-label={t("timers.reset", { label: tm.label })}
              >
                {t("timers.resetShort")}
              </Button>
              <IconButton
                aria-label={t("timers.remove", { label: tm.label })}
                onClick={() => void send(cmd.removeTimer(tm.id))}
              >
                <IconClose size={14} />
              </IconButton>
            </li>
          );
        })}
        {p.view.timers.length === 0 && <li className="muted small">{t("timers.none")}</li>}
      </ul>
      <div className="row gap-8 wrap">
        <TextInput
          aria-label={t("timers.label")}
          placeholder={t("timers.label")}
          value={label}
          onChange={(e) => setLabel(e.target.value)}
          style={{ flex: "1 1 100px" }}
          className="hs-input--bg"
        />
        <Select
          aria-label={t("timers.kind")}
          value={kind}
          onChange={(e) => setKind(e.target.value as TimerKind)}
          className="hs-select--bg"
        >
          <option value="countdown">{t("timers.countdown")}</option>
          <option value="stopwatch">{t("timers.stopwatch")}</option>
        </Select>
        {kind === "countdown" && (
          <TextInput
            aria-label={t("timers.minutes")}
            inputMode="numeric"
            mono
            value={minutes}
            onChange={(e) => setMinutes(e.target.value.replace(/\D/g, ""))}
            style={{ width: 64 }}
            className="hs-input--bg"
          />
        )}
        <Button size="sm" onClick={add}>
          {t("timers.add")}
        </Button>
      </div>
    </section>
  );
}
