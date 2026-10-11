import { commands } from "@hollyshow/protocol";
import { SlideView, defaultTheme } from "@hollyshow/slide-renderer";
import { Button, LiveBadge, Logo, PreviewBadge } from "@hollyshow/ui";
import { useTranslation } from "react-i18next";
import { inTauri, useApi } from "../api/context";
import { usePresentation } from "../hooks/usePresentation";

export function Operator() {
  const { t } = useTranslation();
  const api = useApi();
  const p = usePresentation(api);
  const send = (c: Parameters<typeof api.dispatch>[0]) => void api.dispatch(c);

  if (!p) return null;
  const { view } = p;

  return (
    <div className="app-shell">
      <header className="topbar">
        <Logo />
        <h1>{t("app.name")}</h1>
        <span className="spacer" />
        {!inTauri() && <span className="muted">{t("operator.mock")}</span>}
        <span className="mono muted">v{view.version}</span>
      </header>

      <main className="panes">
        <section className="pane live">
          <LiveBadge>{t("operator.live").toUpperCase()}</LiveBadge>
          <div className="screen">
            <SlideView
              slide={view.live?.slide ?? null}
              theme={defaultTheme}
              hideText={view.clear || view.blackout}
            />
          </div>
        </section>
        <section className="pane preview">
          <PreviewBadge>{t("operator.preview").toUpperCase()}</PreviewBadge>
          <div className="screen">
            <SlideView slide={view.preview?.slide ?? null} theme={defaultTheme} />
          </div>
        </section>
      </main>

      <footer className="controls">
        <Button onClick={() => send(commands.prev(view.version))}>{t("operator.prev")}</Button>
        <Button variant="primary" onClick={() => send(commands.next(view.version))}>
          {t("operator.next")}
        </Button>
        <span className="spacer" />
        <Button onClick={() => send(commands.clear(!view.clear))}>{t("operator.clear")}</Button>
        <Button onClick={() => send(commands.logo(!view.logo))}>{t("operator.logo")}</Button>
        <Button variant="danger" onClick={() => send(commands.blackout(!view.blackout))}>
          {t("operator.blackout")}
        </Button>
        <Button onClick={() => send(commands.serviceMode(!view.serviceMode))}>
          {t("operator.serviceMode")}
        </Button>
      </footer>
    </div>
  );
}
