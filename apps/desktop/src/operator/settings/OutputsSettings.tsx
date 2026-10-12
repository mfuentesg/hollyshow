import type { MonitorInfo, OutputId, OutputLook } from "@hollyshow/protocol";
import { Button, Field, Select, SwitchRow } from "@hollyshow/ui";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useHost } from "../../api/context";
import { useToasts } from "../../hooks/useToasts";
import { cmd } from "../../lib/cmd";
import { useOperator } from "../OperatorContext";

export function monitorLabel(
  m: MonitorInfo,
  t: (k: string, o?: Record<string, unknown>) => string,
): string {
  return t("settings.outputs.monitorLabel", {
    n: m.index + 1,
    name: m.name ?? t("settings.outputs.unnamed"),
    w: m.width,
    h: m.height,
    primary: m.isPrimary ? ` · ${t("settings.outputs.primary")}` : "",
  });
}

/** Outputs and monitor selection, with a look (theme, blackout behavior) per output. */
export function OutputsSettings() {
  const { t } = useTranslation();
  const host = useHost();
  const op = useOperator();
  const toasts = useToasts();
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [assigned, setAssigned] = useState<Record<string, number | null>>({});
  const outputs = Object.entries(op.p.view.outputs) as [OutputId, OutputLook][];

  const refresh = useCallback(async () => {
    try {
      const [m, a] = await Promise.all([host.listMonitors(), host.outputMonitors()]);
      setMonitors(m);
      setAssigned(a);
    } catch (e) {
      toasts.error(e);
    }
  }, [host, toasts]);
  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [host]);

  const setLook = (id: OutputId, look: OutputLook, patch: Partial<OutputLook>) =>
    void op.send(cmd.setOutputLook(id, { ...look, ...patch }));

  const open = async (id: OutputId, monitor: number | null) => {
    try {
      await host.openOutput(id, monitor);
      await refresh();
    } catch (e) {
      toasts.error(e);
    }
  };

  return (
    <div className="stack gap-16">
      <header>
        <h1>{t("settings.outputs.title")}</h1>
        <p className="muted">{t("settings.outputs.intro")}</p>
      </header>

      {monitors.length === 0 && <p className="muted">{t("settings.outputs.noMonitors")}</p>}

      {outputs.map(([id, look]) => {
        const win = assigned[id];
        const isOpen = id in assigned;
        return (
          <section key={id} className="card" aria-labelledby={`out-${id}`}>
            <div className="row gap-8 wrap">
              <h2 id={`out-${id}`} className="card__title grow">
                {t(`outputs.kind.${look.kind}`, { defaultValue: id })}
                <span className="muted small mono"> · {id}</span>
              </h2>
              <span className="muted small">
                {isOpen ? t("settings.outputs.windowOpen") : t("settings.outputs.windowClosed")}
              </span>
            </div>
            <div className="song-fields">
              <Field label={t("settings.outputs.monitor")} className="span-2">
                {(fid) => (
                  <Select
                    id={fid}
                    value={win === undefined || win === null ? "auto" : String(win)}
                    onChange={(e) =>
                      void open(id, e.target.value === "auto" ? null : Number(e.target.value))
                    }
                  >
                    <option value="auto">{t("settings.outputs.auto")}</option>
                    {monitors.map((m) => (
                      <option key={m.index} value={m.index}>
                        {monitorLabel(m, t)}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
              <Field label={t("settings.outputs.theme")} className="span-2">
                {(fid) => (
                  <Select
                    id={fid}
                    value={look.themeId ?? ""}
                    onChange={(e) => setLook(id, look, { themeId: e.target.value || null })}
                  >
                    <option value="">{t("service.themeDefault")}</option>
                    {op.themes.map((th) => (
                      <option key={th.id} value={th.id}>
                        {th.name}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
            </div>
            <SwitchRow
              label={t("settings.outputs.enabled")}
              checked={look.enabled}
              onChange={(v) => setLook(id, look, { enabled: v })}
            />
            <SwitchRow
              label={t("settings.outputs.followsBlackout")}
              checked={look.followsBlackout}
              onChange={(v) => setLook(id, look, { followsBlackout: v })}
            />
            <div className="row gap-8">
              <Button size="sm" onClick={() => void open(id, win ?? null)}>
                {isOpen ? t("settings.outputs.reopen") : t("settings.outputs.open")}
              </Button>
              {isOpen && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    void host.closeOutput(id).then(refresh).catch(toasts.error);
                  }}
                >
                  {t("settings.outputs.close")}
                </Button>
              )}
            </div>
          </section>
        );
      })}

      <section className="card" aria-labelledby="out-add">
        <h2 id="out-add" className="card__title">
          {t("settings.outputs.addTitle")}
        </h2>
        <div className="row gap-8 wrap">
          {(["secondary", "lowerThird", "stage"] as const)
            .filter((k) => k !== "stage" || !outputs.some(([, l]) => l.kind === "stage"))
            .map((k) => (
              <Button
                key={k}
                size="sm"
                onClick={() => {
                  const taken = new Set(outputs.map(([id]) => id));
                  let n = 1;
                  const base = k === "secondary" ? "aux" : k === "lowerThird" ? "lower" : "stage";
                  while (taken.has(k === "stage" ? base : `${base}${n}`)) n++;
                  const id = k === "stage" ? base : `${base}${n}`;
                  void op.send(
                    cmd.setOutputLook(id, {
                      kind: k,
                      themeId: null,
                      enabled: true,
                      followsBlackout: k !== "stage",
                    }),
                  );
                }}
              >
                + {t(`outputs.kind.${k}`)}
              </Button>
            ))}
        </div>
      </section>
    </div>
  );
}
