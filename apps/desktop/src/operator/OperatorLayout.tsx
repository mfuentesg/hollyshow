import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Outlet } from "react-router-dom";
import { useHost } from "../api/context";
import { ToastProvider } from "../hooks/useToasts";
import { CommandPalette } from "./CommandPalette";
import { Footer } from "./Footer";
import { OperatorProvider, useOperator } from "./OperatorContext";
import { ShortcutsHelp } from "./ShortcutsHelp";
import { TemplateDialog } from "./TemplateDialog";
import { TopBar } from "./TopBar";
import { useOperatorKeys } from "./useOperatorKeys";

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
  const [help, setHelp] = useState(false);
  const [templates, setTemplates] = useState(false);
  useOperatorKeys(op, () => setHelp(true));
  return (
    <div className="op-shell">
      <TopBar />
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
        <Shell />
      </OperatorProvider>
    </ToastProvider>
  );
}
