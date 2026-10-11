import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Outlet } from "react-router-dom";
import { inTauri, useHost } from "../api/context";
import { useHostEvents } from "../hooks/useHostEvents";
import { ToastProvider, useToasts } from "../hooks/useToasts";
import { CommandPalette } from "./CommandPalette";
import { Footer } from "./Footer";
import { OperatorProvider, useOperator } from "./OperatorContext";
import { ShortcutsHelp } from "./ShortcutsHelp";
import { TemplateDialog } from "./TemplateDialog";
import { TopBar } from "./TopBar";
import {
  UpdateOverlay,
  UpdateProvider,
  createMemoryUpdateApi,
  createTauriUpdateApi,
} from "./update";
import { useOperatorKeys } from "./useOperatorKeys";

/** Update flow of the desktop shell; a harmless in-memory one in a plain browser. */
function UpdateGate({ children }: { children: ReactNode }) {
  const op = useOperator();
  const api = useMemo(() => (inTauri() ? createTauriUpdateApi() : createMemoryUpdateApi().api), []);
  // The host refuses to restart during service mode or with a live output; mirror that here so
  // the banner never offers a button that cannot work.
  return (
    <UpdateProvider api={api} busy={op.p.view.serviceMode}>
      {children}
    </UpdateProvider>
  );
}

/** "Retomar donde estabas" and the other one-off notices the host sends. */
function useHostNotices() {
  const host = useHost();
  const toasts = useToasts();
  const { t } = useTranslation();
  useEffect(() => {
    let alive = true;
    void host
      .startupInfo()
      .then((info) => {
        if (!alive) return;
        if (info.dbRecovered) toasts.push(t("startup.dbRecovered"), "error");
        if (info.crashed) toasts.push(t("startup.crashed"), "info");
        if (info.resumed) {
          const where = info.playlist ?? t("startup.noService");
          toasts.push(
            info.hadLive
              ? t("startup.resumedLive", { playlist: where })
              : t("startup.resumed", { playlist: where }),
            "success",
          );
        }
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [host]);
  useHostEvents({
    closeBlocked: () => toasts.push(t("startup.closeBlocked"), "info"),
    monitors: () => toasts.push(t("startup.monitorsChanged"), "info"),
    outputPaused: () => toasts.push(t("startup.outputPaused"), "error"),
  });
}

function Shell() {
  const op = useOperator();
  const host = useHost();
  const { i18n } = useTranslation();
  useEffect(() => {
    void host
      .getSetting("ui.language")
      .then((l) => (l === "es" || l === "en") && void i18n.changeLanguage(l))
      .catch(() => {});
  }, [host, i18n]);
  useHostNotices();
  const [help, setHelp] = useState(false);
  const [templates, setTemplates] = useState(false);
  useOperatorKeys(op, () => setHelp(true));
  return (
    <div className="op-shell">
      <TopBar />
      <UpdateOverlay />
      <div className="op-body">
        <Outlet />
      </div>
      <Footer />
      {op.paletteOpen && (
        <CommandPalette
          onClose={() => op.setPaletteOpen(false)}
          onOpenHelp={() => setHelp(true)}
          onOpenTemplates={() => setTemplates(true)}
        />
      )}
      {help && <ShortcutsHelp onClose={() => setHelp(false)} />}
      {templates && <TemplateDialog onClose={() => setTemplates(false)} />}
    </div>
  );
}

/** Operator window: providers + chrome (top bar, footer) around the active screen. */
export function OperatorLayout() {
  return (
    <ToastProvider>
      <OperatorProvider>
        <UpdateGate>
          <Shell />
        </UpdateGate>
      </OperatorProvider>
    </ToastProvider>
  );
}
