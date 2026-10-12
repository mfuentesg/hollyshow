import type { Cue } from "@hollyshow/protocol";
import { SlideView } from "@hollyshow/slide-renderer";
import { Button, Logo, TextInput } from "@hollyshow/ui";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { cmd } from "../lib/cmd";
import { TimersPanel } from "./TimersPanel";
import { useOperator } from "./OperatorContext";

function transitionLabel(
  t: ReturnType<typeof useTranslation>["t"],
  kind: string,
  ms: number,
): string {
  const secs = (ms / 1000).toLocaleString(undefined, { maximumFractionDigits: 1 });
  return `${t(`themes.transitions.${kind}`, { defaultValue: kind })} ${secs} s`;
}

function Monitor({ kind, cue }: { kind: "live" | "preview"; cue: Cue | null }) {
  const { t } = useTranslation();
  const { p, themeFor } = useOperator();
  const { view } = p;
  const theme = themeFor(cue?.slide.themeId ?? view.outputs["main"]?.themeId);
  const live = kind === "live";
  const hide = live && (view.clear || view.blackout);
  const label = cue?.slide.label ?? (cue ? "" : t("operator.empty"));
  return (
    <section
      className={`monitor monitor--${kind}`}
      aria-label={live ? t("operator.live") : t("operator.preview")}
    >
      <div className="monitor__head">
        {live ? (
          <span className="hs-badge hs-badge--live">● {t("operator.live").toUpperCase()}</span>
        ) : (
          <span className="monitor__preview-tag">{t("operator.preview").toUpperCase()}</span>
        )}
        <span className="muted small">
          {label}
          {live && cue
            ? ` · ${transitionLabel(t, theme.transition.kind, theme.transition.durationMs)}`
            : ""}
        </span>
      </div>
      <div className="monitor__screen" data-testid={`monitor-${kind}`}>
        <SlideView slide={cue?.slide ?? null} theme={theme} hideText={hide} />
        {live && view.blackout && (
          <div className="monitor__veil" role="status">
            {t("operator.blackout").toUpperCase()}
          </div>
        )}
        {live && !view.blackout && view.logo && (
          <div className="monitor__veil monitor__veil--logo" role="status">
            <Logo size={64} />
          </div>
        )}
      </div>
    </section>
  );
}

/** Right column: preview + live monitors, panic buttons, navigation, stage message, timers. */
export function MonitorsColumn() {
  const { t } = useTranslation();
  const { p, send } = useOperator();
  const { view } = p;
  const [msg, setMsg] = useState(view.stageMessage?.text ?? "");
  const remoteMsg = view.stageMessage?.text ?? "";
  useEffect(() => setMsg(remoteMsg), [remoteMsg]);

  return (
    <aside className="panel panel--right" aria-label={t("operator.monitors")}>
      <Monitor kind="preview" cue={view.preview} />
      <Monitor kind="live" cue={view.live} />

      <div className="panic" role="group" aria-label={t("operator.panic")}>
        <Button
          panic
          className="hs-btn--black"
          aria-pressed={view.blackout}
          onClick={() => void send(cmd.blackout(!view.blackout))}
          title="B"
        >
          {t("operator.blackout")}
        </Button>
        <Button
          panic
          aria-pressed={view.clear}
          onClick={() => void send(cmd.clear(!view.clear))}
          title="X"
        >
          {t("operator.clear")}
        </Button>
        <Button
          panic
          aria-pressed={view.logo}
          onClick={() => void send(cmd.logo(!view.logo))}
          title="L"
        >
          {t("operator.logo")}
        </Button>
      </div>

      <div className="row gap-8">
        <Button style={{ flex: 1 }} onClick={() => void send(cmd.prev(view))} title="←">
          ◀ {t("operator.prev")}
        </Button>
        <Button
          style={{ flex: 1 }}
          disabled={!view.preview}
          onClick={() => void send(cmd.takePreview())}
          title="Enter"
        >
          {t("operator.takePreview")}
        </Button>
        <Button
          variant="primary"
          style={{ flex: 1 }}
          onClick={() => void send(cmd.next(view))}
          title="→ / Space"
        >
          {t("operator.next")} ▶
        </Button>
      </div>

      <form
        className="stack"
        onSubmit={(e) => {
          e.preventDefault();
          void send(cmd.stageMessage(msg.trim() ? msg.trim() : null));
        }}
      >
        <label htmlFor="stagemsg" className="muted">
          {t("operator.stageMessage")}
        </label>
        <div className="row gap-8">
          <TextInput
            id="stagemsg"
            className="hs-input--bg"
            style={{ flex: 1 }}
            value={msg}
            maxLength={280}
            onChange={(e) => setMsg(e.target.value)}
            placeholder={t("operator.stageMessagePlaceholder")}
          />
          <Button type="submit">{t("operator.send")}</Button>
          <Button
            variant="ghost"
            disabled={!view.stageMessage}
            onClick={() => {
              setMsg("");
              void send(cmd.stageMessage(null));
            }}
          >
            {t("common.clear")}
          </Button>
        </div>
      </form>

      {/* TODO(post-v1): «Sugerencia por voz» (AI voice assistant) is out of scope for v1. */}
      <TimersPanel />
    </aside>
  );
}
