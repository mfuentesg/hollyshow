import type { MonitorInfo } from "@hollyshow/protocol";
import { Button } from "@hollyshow/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useHost } from "../api/context";
import { useToasts } from "../hooks/useToasts";
import { monitorLabel } from "./settings/OutputsSettings";

/**
 * One-click recovery for a paused output (its monitor vanished): a "Reasignar" button with a
 * small menu of the monitors available right now plus "Esta pantalla" (the operator's own). The
 * pick re-opens the output window there through the regular output-assignment command.
 */
export function ReassignOutput({ outputs, onDone }: { outputs: string[]; onDone: () => void }) {
  const { t } = useTranslation();
  const host = useHost();
  const toasts = useToasts();
  const [open, setOpen] = useState(false);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [here, setHere] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);

  if (outputs.length === 0) return null;

  const toggle = async () => {
    if (open) return setOpen(false);
    setOpen(true);
    try {
      const [m, h] = await Promise.all([host.listMonitors(), host.operatorMonitor()]);
      setMonitors(m);
      setHere(h);
    } catch (e) {
      toasts.error(e);
      setOpen(false);
    }
  };

  const assign = async (monitor: number) => {
    setBusy(true);
    try {
      for (const id of outputs) await host.openOutput(id, monitor);
      toasts.push(t("startup.reassigned", { n: monitor + 1 }), "success");
      onDone();
    } catch (e) {
      toasts.error(e);
      setBusy(false);
    }
  };

  return (
    <div className="toast__actions">
      <Button size="sm" aria-haspopup="menu" aria-expanded={open} onClick={() => void toggle()}>
        {t("startup.reassign")}
      </Button>
      {open && (
        <ul className="toast__menu" role="menu" aria-label={t("startup.reassignMenu")}>
          {here !== null && monitors.some((m) => m.index === here) && (
            <li role="none">
              <button
                type="button"
                role="menuitem"
                className="toast__item"
                disabled={busy}
                onClick={() => void assign(here)}
              >
                {t("startup.thisScreen")}
              </button>
            </li>
          )}
          {monitors.map((m) => (
            <li key={m.index} role="none">
              <button
                type="button"
                role="menuitem"
                className="toast__item"
                disabled={busy}
                onClick={() => void assign(m.index)}
              >
                {monitorLabel(m, t)}
              </button>
            </li>
          ))}
          {monitors.length === 0 && (
            <li className="muted small">{t("settings.outputs.noMonitors")}</li>
          )}
        </ul>
      )}
    </div>
  );
}
