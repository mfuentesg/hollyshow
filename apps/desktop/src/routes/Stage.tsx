import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "../api/context";
import { usePresentation } from "../hooks/usePresentation";

export function Stage() {
  const { t } = useTranslation();
  const p = usePresentation(useApi());
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const i = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(i);
  }, []);
  if (!p) return null;
  const { view } = p;

  return (
    <div className="stage-root">
      <div className="mono" style={{ gridColumn: "1 / -1", fontSize: 40 }}>
        {now.toLocaleTimeString()}
      </div>
      <section>
        <h2 className="muted">{t("stage.now")}</h2>
        {view.live?.slide.lines.map((l, i) => (
          <div key={i} style={{ fontSize: 56, fontWeight: 600 }}>
            {l}
          </div>
        ))}
      </section>
      <section>
        <h2 className="muted">{t("stage.next")}</h2>
        {view.preview?.slide.lines.map((l, i) => (
          <div key={i} style={{ fontSize: 32 }} className="muted">
            {l}
          </div>
        ))}
        {view.stageMessage && (
          <div style={{ marginTop: 24, fontSize: 36, color: "var(--hs-amber)" }}>
            {view.stageMessage.text}
          </div>
        )}
      </section>
    </div>
  );
}
