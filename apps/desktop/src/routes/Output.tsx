import { SlideView, defaultTheme } from "@hollyshow/slide-renderer";
import { Logo } from "@hollyshow/ui";
import { useParams } from "react-router-dom";
import { useApi } from "../api/context";
import { usePresentation } from "../hooks/usePresentation";

/**
 * Projector / OBS window. "Dumb" on purpose (PLAN §3.1.5): renders only the last received view
 * and never depends on the operator window being alive.
 */
export function Output() {
  const { id = "main" } = useParams();
  const p = usePresentation(useApi());
  if (!p) return <div className="output-root" />;

  const { view } = p;
  const look = view.outputs[id];
  const blackout = view.blackout && (look?.followsBlackout ?? true);
  const transparent = look?.kind === "lowerThird";

  return (
    <div className="output-root" style={transparent ? { background: "transparent" } : undefined}>
      {blackout ? null : view.logo ? (
        <div style={{ display: "grid", placeItems: "center", height: "100%" }}>
          <Logo size={240} />
        </div>
      ) : (
        <SlideView
          slide={view.live?.slide ?? null}
          theme={defaultTheme}
          hideText={view.clear}
          transparent={transparent}
        />
      )}
    </div>
  );
}
