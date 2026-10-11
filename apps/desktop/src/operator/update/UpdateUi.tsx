import { Button, Field, Modal, Select, SwitchRow } from "@hollyshow/ui";
import { useTranslation } from "react-i18next";
import { Markdown } from "./markdown";
import { UPDATE_NS } from "./i18n";
import { percent, releaseOf } from "./state";
import { useUpdate } from "./UpdateContext";
import type { UpdateChannel } from "./types";

/** Discreet footer indicator: download progress, "ready" or "installs on exit". Renders nothing otherwise. */
export function UpdateFooterIndicator() {
  const { t } = useTranslation(UPDATE_NS);
  const { state, busy, openNotes } = useUpdate();
  if (state.state === "checking") {
    return <span className="upd-footer">{t("checking")}</span>;
  }
  if (state.state === "downloading") {
    const pct = percent(state.downloaded, state.total);
    return (
      <span className="upd-footer">
        {pct === null
          ? t("downloading", { version: state.version })
          : t("downloadingPercent", { version: state.version, percent: pct })}
        <progress
          className="upd-progress"
          aria-label={t("progressLabel")}
          max={100}
          value={pct ?? undefined}
        />
      </span>
    );
  }
  if (state.state === "ready") {
    return (
      <button type="button" className="upd-footer upd-footer--link" onClick={openNotes}>
        {busy ? `${t("installOnExit")} · ${state.version}` : t("ready", { version: state.version })}
      </button>
    );
  }
  return null;
}

/** Non-intrusive notice once an update is downloaded. */
export function UpdateBanner() {
  const { t } = useTranslation(UPDATE_NS);
  const { state, busy, bannerDismissed, installNow, openNotes, dismissBanner } = useUpdate();
  if (state.state !== "ready" || bannerDismissed) return null;
  return (
    <div className="upd-banner" role="status" aria-label={t("banner")}>
      <span className="upd-banner__text">
        <strong>{t("ready", { version: state.version })}</strong>
        {busy && (
          <>
            {" · "}
            <span title={t("installOnExitHint")}>{t("installOnExit")}</span>
          </>
        )}
      </span>
      {!busy && (
        <Button variant="primary" size="sm" onClick={() => void installNow()}>
          {t("restart")}
        </Button>
      )}
      <Button size="sm" onClick={openNotes}>
        {t("notes")}
      </Button>
      <Button variant="ghost" size="sm" onClick={dismissBanner}>
        {t("later")}
      </Button>
    </div>
  );
}

/** Release notes of the pending version (Markdown rendered without raw HTML). */
export function ReleaseNotesModal() {
  const { t, i18n } = useTranslation(UPDATE_NS);
  const { state, notesOpen, closeNotes, busy, installNow } = useUpdate();
  const rel = releaseOf(state);
  if (!notesOpen || !rel) return null;
  const date =
    rel.date !== null
      ? new Date(rel.date).toLocaleDateString(i18n.language, { dateStyle: "long" })
      : null;
  return (
    <Modal
      title={t("notesTitle", { version: rel.version })}
      onClose={closeNotes}
      closeLabel={t("close")}
      width={640}
      footer={
        <>
          {state.state === "ready" && !busy && (
            <Button
              variant="primary"
              onClick={() => {
                closeNotes();
                void installNow();
              }}
            >
              {t("restart")}
            </Button>
          )}
          <Button onClick={closeNotes}>{t("close")}</Button>
        </>
      }
    >
      {date && <p className="muted">{t("notesDate", { date })}</p>}
      {rel.notes.trim() ? (
        <Markdown source={rel.notes} className="md upd-notes" />
      ) : (
        <p className="muted">{t("notesEmpty")}</p>
      )}
    </Modal>
  );
}

/** Mount once inside `UpdateProvider`, e.g. next to the operator footer. */
export function UpdateOverlay() {
  return (
    <>
      <UpdateBanner />
      <ReleaseNotesModal />
    </>
  );
}

/** "Ajustes → Actualizaciones": automatic check, automatic download, channel and manual check. */
export function UpdateSettings() {
  const { t } = useTranslation(UPDATE_NS);
  const { settings, saveSettings, checkNow, currentVersion, state, downloadNow } = useUpdate();
  const checking = state.state === "checking" || state.state === "downloading";
  return (
    <div className="stack gap-16">
      <header>
        <h1>{t("title")}</h1>
        <p className="muted">{t("intro")}</p>
      </header>
      <SwitchRow
        label={t("settings.autoCheck")}
        checked={settings.autoCheck}
        onChange={(autoCheck) => void saveSettings({ ...settings, autoCheck })}
      />
      <SwitchRow
        label={t("settings.autoDownload")}
        checked={settings.autoDownload}
        onChange={(autoDownload) => void saveSettings({ ...settings, autoDownload })}
      />
      <Field
        label={t("settings.channel")}
        hint={settings.channel === "beta" ? t("settings.betaHint") : undefined}
        style={{ maxWidth: 320 }}
      >
        {(id) => (
          <Select
            id={id}
            value={settings.channel}
            onChange={(e) =>
              void saveSettings({ ...settings, channel: e.target.value as UpdateChannel })
            }
          >
            <option value="stable">{t("settings.stable")}</option>
            <option value="beta">{t("settings.beta")}</option>
          </Select>
        )}
      </Field>
      <div className="row gap-12">
        <Button disabled={checking} onClick={() => void checkNow()}>
          {t("checkNow")}
        </Button>
        {state.state === "available" && (
          <Button variant="primary" onClick={() => void downloadNow()}>
            {t("downloadNow")} {state.version}
          </Button>
        )}
        {currentVersion && (
          <span className="muted">{t("currentVersion", { version: currentVersion })}</span>
        )}
      </div>
      {state.state === "error" && (
        <p role="alert" className="upd-error">
          {t("checkFailed", { message: state.message })}
        </p>
      )}
    </div>
  );
}
