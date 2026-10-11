import { useTranslation } from "react-i18next";
import { useNow } from "../hooks/useNow";
import { useOperator } from "./OperatorContext";

function pinLabel(pin: string): string {
  return pin.length === 6 ? `${pin.slice(0, 3)} ${pin.slice(3)}` : pin;
}

/** Health strip: output fps, CPU/RAM, remote URL + PIN and the wall clock. */
export function Footer() {
  const { t } = useTranslation();
  const { health, pairing } = useOperator();
  const now = useNow(1000);
  const clock = new Date(now).toLocaleTimeString(undefined, { hour12: false });
  return (
    <footer className="footer">
      <span>
        {health
          ? t("footer.output", { fps: Math.round(health.fps), dropped: health.droppedFrames })
          : t("footer.noHealth")}
      </span>
      {health && (
        <span>
          {t("footer.system", {
            cpu: Math.round(health.cpuPercent),
            ram: Math.round(health.ramMb),
          })}
        </span>
      )}
      <span>
        {pairing?.enabled ? (
          <>
            {t("footer.remote")}:{" "}
            <span className="mono strong">{pairing.url.replace(/^https?:\/\//, "")}</span> · PIN{" "}
            <span className="mono strong">{pinLabel(pairing.pin)}</span>
          </>
        ) : (
          t("footer.remoteOff")
        )}
      </span>
      <span className="push-right mono strong" aria-label={t("footer.clock")}>
        {clock}
      </span>
    </footer>
  );
}
