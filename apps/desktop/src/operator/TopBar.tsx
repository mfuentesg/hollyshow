import { IconPhone, IconSearch, Kbd, Logo, StatusDot } from "@hollyshow/ui";
import { useTranslation } from "react-i18next";
import { NavLink, useNavigate } from "react-router-dom";
import { cmd } from "../lib/cmd";
import { useNow } from "../hooks/useNow";
import { useOperator } from "./OperatorContext";

const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

const NAV = [
  { to: "/operator", key: "nav.service", end: true },
  { to: "/operator/songs", key: "nav.songs", end: false },
  { to: "/operator/bible", key: "nav.bible", end: false },
  { to: "/operator/themes", key: "nav.themes", end: false },
  { to: "/operator/settings", key: "nav.settings", end: false },
] as const;

export function TopBar() {
  const { t } = useTranslation();
  const op = useOperator();
  const nav = useNavigate();
  const now = useNow(1000);
  const { p, playlist } = op;
  const { view } = p;
  const connected = op.devices.filter((d) => d.connected).length;
  const savedAgo = op.savedAt ? Math.max(0, Math.round((now - op.savedAt) / 1000)) : null;
  const outputs = Object.entries(view.outputs);

  return (
    <header className="topbar">
      <div className="brand">
        <Logo size={28} />
        <span className="brand__name">{t("app.name")}</span>
      </div>

      <nav aria-label={t("nav.sections")} className="hs-tabs">
        {NAV.map((n) => (
          <NavLink key={n.to} to={n.to} end={n.end} className="hs-tab">
            {t(n.key)}
          </NavLink>
        ))}
      </nav>

      <div className="svc-title">
        <span className="svc-title__name">{playlist?.name ?? t("service.noService")}</span>
        <span className="muted small">
          {playlist?.isTemplate ? `${t("service.templateTag")} · ` : ""}
          {savedAgo === null ? t("service.notEdited") : t("service.saved", { secs: savedAgo })}
        </span>
      </div>

      <button
        type="button"
        className="searchbox searchbox--button"
        aria-haspopup="dialog"
        aria-label={t("palette.open")}
        onClick={() => op.setPaletteOpen(true)}
      >
        <IconSearch />
        <span className="searchbox__ph">{t("topbar.searchPlaceholder")}</span>
        <Kbd>{isMac ? "⌘K" : "Ctrl K"}</Kbd>
      </button>

      <div className="status" role="group" aria-label={t("topbar.status")}>
        {outputs.map(([id, look]) => (
          <button
            key={id}
            type="button"
            className="hs-pill hs-pill--button"
            onClick={() => nav("/operator/settings/outputs")}
            title={t("settings.outputs.title")}
          >
            <StatusDot state={look.enabled ? "ok" : "off"} />
            {t(`outputs.kind.${look.kind}`, { defaultValue: id })}
            <span className="hs-sr-only">{look.enabled ? t("outputs.on") : t("outputs.off")}</span>
          </button>
        ))}
        <button
          type="button"
          className="hs-pill hs-pill--button"
          onClick={() => nav("/operator/settings/remote")}
          title={t("settings.remote.title")}
        >
          <IconPhone size={14} />
          {t("topbar.remotes", { count: connected })}
        </button>
        <button
          type="button"
          className={`hs-pill hs-pill--button${view.serviceMode ? " hs-pill--amber" : ""}`}
          aria-pressed={view.serviceMode}
          title={t("topbar.serviceModeHint")}
          onClick={() => void op.send(cmd.serviceMode(!view.serviceMode))}
        >
          {t("operator.serviceMode")}
        </button>
      </div>
    </header>
  );
}
