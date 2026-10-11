import { Button, SwitchRow } from "@hollyshow/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useHost } from "../../api/context";
import { useNow } from "../../hooks/useNow";
import { useToasts } from "../../hooks/useToasts";
import { useOperator } from "../OperatorContext";
import { QrCode } from "./qr";

function ago(ms: number, t: (k: string, o?: Record<string, unknown>) => string): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return t("settings.remote.agoSecs", { n: s });
  if (s < 3600) return t("settings.remote.agoMins", { n: Math.round(s / 60) });
  return t("settings.remote.agoHours", { n: Math.round(s / 3600) });
}

/** Remote pairing: QR + PIN and the paired device list (with kick). */
export function RemoteSettings() {
  const { t } = useTranslation();
  const host = useHost();
  const op = useOperator();
  const toasts = useToasts();
  const now = useNow(1000);
  const [busy, setBusy] = useState(false);
  const { pairing, devices } = op;

  const guard = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await fn();
      await op.refreshHost();
    } catch (e) {
      toasts.error(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="stack gap-16">
      <header>
        <h1>{t("settings.remote.title")}</h1>
        <p className="muted">{t("settings.remote.intro")}</p>
      </header>

      <SwitchRow
        label={t("settings.remote.enabled")}
        checked={pairing?.enabled ?? false}
        disabled={!pairing || busy}
        onChange={(v) => void guard(() => host.setRemoteEnabled(v))}
      />

      {pairing?.enabled && (
        <section className="card pairing" aria-labelledby="pair-h">
          <h2 id="pair-h" className="card__title">
            {t("settings.remote.pairTitle")}
          </h2>
          <div className="row gap-16 wrap">
            <QrCode
              value={pairing.url}
              label={t("settings.remote.qrLabel", { url: pairing.url })}
            />
            <div className="stack">
              <span className="muted small">{t("settings.remote.url")}</span>
              <span className="mono strong">{pairing.url}</span>
              <span className="muted small">PIN</span>
              <span className="pin mono" aria-label={`PIN ${pairing.pin.split("").join(" ")}`}>
                {pairing.pin.length === 6
                  ? `${pairing.pin.slice(0, 3)} ${pairing.pin.slice(3)}`
                  : pairing.pin}
              </span>
              {pairing.pinExpiresMs !== null && (
                <span className="muted small">
                  {t("settings.remote.expires", {
                    secs: Math.max(0, Math.round((pairing.pinExpiresMs - now) / 1000)),
                  })}
                </span>
              )}
              <Button
                size="sm"
                disabled={busy}
                onClick={() => void guard(() => host.regeneratePin())}
              >
                {t("settings.remote.regenerate")}
              </Button>
            </div>
          </div>
        </section>
      )}

      <section className="stack" aria-labelledby="dev-h">
        <h2 id="dev-h" className="subhead">
          {t("settings.remote.devices")}
        </h2>
        {devices.length === 0 && <p className="muted">{t("settings.remote.noDevices")}</p>}
        <ul className="picklist">
          {devices.map((d) => (
            <li key={d.id} className="picklist__row device">
              <span className={`hs-dot${d.connected ? "" : " hs-dot--off"}`} aria-hidden="true" />
              <span className="grow">
                <strong>{d.name}</strong>
                <span className="muted small">
                  {" "}
                  · {t(`settings.remote.roles.${d.role}`)} ·{" "}
                  {d.connected ? t("settings.remote.connected") : ago(now - d.lastSeenMs, t)}
                </span>
              </span>
              <Button
                size="sm"
                variant="danger"
                disabled={busy}
                aria-label={t("settings.remote.kickLabel", { name: d.name })}
                onClick={() =>
                  void guard(async () => {
                    await host.kickDevice(d.id);
                    toasts.push(t("settings.remote.kicked", { name: d.name }), "info");
                  })
                }
              >
                {t("settings.remote.kick")}
              </Button>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
