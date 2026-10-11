import type { ConnectionStatus, Cue, Role } from "@hollyshow/protocol";
import { useEffect, useState, type ReactNode, type SVGProps } from "react";
import { useT } from "../lib/i18n";
import { slideText } from "../lib/slideText";

type IconProps = SVGProps<SVGSVGElement> & { size?: number };
const Svg = ({ size = 20, children, ...p }: IconProps) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
    {...p}
  >
    {children}
  </svg>
);
export const IconSlides = (p: IconProps) => (
  <Svg {...p}>
    <rect x="3" y="5" width="18" height="12" rx="2" />
    <path d="M8 21h8" />
  </Svg>
);
export const IconList = (p: IconProps) => (
  <Svg {...p}>
    <path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" />
  </Svg>
);
export const IconBook = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 4h7a3 3 0 0 1 3 3v13a2 2 0 0 0-2-2H4zM20 4h-5" />
    <path d="M20 4v14h-6" />
  </Svg>
);
export const IconChat = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 5h16v11H8l-4 4z" />
  </Svg>
);
export const IconLeft = (p: IconProps) => (
  <Svg strokeWidth={2.4} {...p}>
    <path d="M15 5l-7 7 7 7" />
  </Svg>
);
export const IconRight = (p: IconProps) => (
  <Svg strokeWidth={2.4} {...p}>
    <path d="M9 5l7 7-7 7" />
  </Svg>
);
export const IconSearch = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="11" cy="11" r="7" />
    <path d="M20 20l-3.5-3.5" />
  </Svg>
);
export const IconMusic = (p: IconProps) => (
  <Svg {...p}>
    <path d="M9 18V6l11-2v12" />
    <circle cx="6.5" cy="18" r="2.5" />
    <circle cx="17.5" cy="16" r="2.5" />
  </Svg>
);

export function Logo({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 40 40" aria-hidden="true">
      <rect x="1" y="1" width="38" height="38" rx="10" fill="#F2B544" />
      <path d="M20 9c3 4 5 6.5 5 10a5 5 0 0 1-10 0c0-3.5 2-6 5-10z" fill="#1A1405" />
      <path d="M12 30h16" stroke="#1A1405" strokeWidth="2.5" strokeLinecap="round" />
    </svg>
  );
}

const DOT: Record<ConnectionStatus, string> = {
  open: "dot ok",
  connecting: "dot warn",
  reconnecting: "dot warn pulse",
  closed: "dot bad",
};

export function StatusDot({ status }: { status: ConnectionStatus }) {
  const t = useT();
  return <span className={DOT[status]} role="img" aria-label={t(`conn.${status}`)} />;
}

export function Clock() {
  // Local wall-clock time (the host clock is only needed for timers).
  const [d, setD] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setD(new Date()), 15_000);
    return () => clearInterval(id);
  }, []);
  return (
    <span className="mono clock">
      {String(d.getHours()).padStart(2, "0")}:{String(d.getMinutes()).padStart(2, "0")}
    </span>
  );
}

export interface HeaderProps {
  title: string;
  status: ConnectionStatus;
  serverName: string;
  role: Role | null;
  right?: ReactNode;
}

export function Header({ title, status, serverName, role, right }: HeaderProps) {
  const t = useT();
  const parts = [t(`conn.${status}`), serverName, role ? t(`role.${role}`) : ""].filter(Boolean);
  return (
    <header className="head">
      <StatusDot status={status} />
      <div className="head-text">
        <span className="head-title">{title}</span>
        <span className="head-sub">{parts.join(" · ")}</span>
      </div>
      {right ?? <Clock />}
    </header>
  );
}

export function OfflineBanner({ status }: { status: ConnectionStatus }) {
  const t = useT();
  if (status === "open") return null;
  return (
    <div className="offline" role="alert">
      {t("conn.offlineBanner")}
    </div>
  );
}

/** Small 16:9 view of what is on a cue (text only; the projector renders the real thing). */
export function MiniPreview({
  cue,
  kind,
  blackout,
  clear,
  logo,
}: {
  cue: Cue | null;
  kind: "live" | "preview";
  blackout?: boolean;
  clear?: boolean;
  logo?: boolean;
}) {
  const t = useT();
  const text = blackout || clear || logo ? "" : slideText(cue?.slide);
  const size = text.length > 160 ? 11 : text.length > 90 ? 13 : 15;
  const badge = kind === "live" ? (blackout ? t("op.black") : `● ${t("op.live")}`) : "PREVIEW";
  return (
    <div className={`mini ${kind}${blackout ? " black" : ""}`} aria-label={badge}>
      <span className={`badge ${kind}`}>{badge}</span>
      {logo && !blackout ? (
        <Logo size={36} />
      ) : (
        <span className="mini-text" style={{ fontSize: size }}>
          {text || (cue || blackout ? "" : t("op.nothing"))}
        </span>
      )}
    </div>
  );
}
