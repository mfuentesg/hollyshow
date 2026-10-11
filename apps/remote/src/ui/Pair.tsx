import { useState } from "react";
import { getLang, setLang, useLang, useT, type MessageKey } from "../lib/i18n";
import { PairError } from "../net/pairing";
import { Logo } from "./common";

export interface PairScreenProps {
  /** Error carried over from a failed automatic pairing (QR) or a rejected session. */
  initialError?: MessageKey | null;
  busy?: boolean;
  deviceName: string;
  onDeviceName(name: string): void;
  onPin(pin: string): Promise<void>;
  onFollow(): void;
}

export function errorKey(e: unknown): MessageKey {
  if (e instanceof PairError) {
    switch (e.failure) {
      case "invalid":
        return "pair.err.invalid";
      case "locked":
      case "rate_limited":
        return "pair.err.locked";
      case "network":
        return "pair.err.network";
    }
  }
  return "pair.err.generic";
}

export function PairScreen({
  initialError = null,
  busy = false,
  deviceName,
  onDeviceName,
  onPin,
  onFollow,
}: PairScreenProps) {
  const t = useT();
  const lang = useLang();
  const [pin, setPin] = useState("");
  const [err, setErr] = useState<MessageKey | null>(initialError);
  const [working, setWorking] = useState(false);
  const loading = busy || working;

  const submit = async () => {
    if (pin.length !== 6 || loading) return;
    setWorking(true);
    setErr(null);
    try {
      await onPin(pin);
    } catch (e) {
      setErr(errorKey(e));
    } finally {
      setWorking(false);
    }
  };

  return (
    <main className="pair">
      <div className="pair-top">
        <Logo size={44} />
        <button className="chip" onClick={() => setLang(getLang() === "es" ? "en" : "es")}>
          {t("lang.switch")}
        </button>
      </div>
      <h1>{t("pair.title")}</h1>
      <p className="muted">{t("pair.hint")}</p>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <label className="field">
          {t("pair.pin")}
          <input
            className="pin mono"
            inputMode="numeric"
            pattern="[0-9]*"
            autoComplete="one-time-code"
            maxLength={6}
            value={pin}
            lang={lang}
            aria-invalid={err !== null}
            aria-describedby={err ? "pair-err" : undefined}
            onChange={(e) => {
              setPin(e.target.value.replace(/\D/g, "").slice(0, 6));
              setErr(null);
            }}
            placeholder="000000"
            autoFocus
          />
        </label>
        {err && (
          <p id="pair-err" className="error" role="alert">
            {t(err)}
          </p>
        )}
        <label className="field">
          {t("pair.device")}
          <input
            className="input"
            maxLength={40}
            value={deviceName}
            onChange={(e) => onDeviceName(e.target.value)}
          />
        </label>
        <button className="cta tall wide" type="submit" disabled={pin.length !== 6 || loading}>
          {loading ? t("pair.checking") : t("pair.connect")}
        </button>
      </form>

      <button className="link" onClick={onFollow}>
        {t("pair.follow")}
      </button>
    </main>
  );
}
