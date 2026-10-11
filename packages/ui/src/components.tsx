import type { ButtonHTMLAttributes, CSSProperties, ReactNode } from "react";

type Variant = "primary" | "secondary" | "danger";

const base: CSSProperties = {
  font: "600 14px var(--hs-font-ui)",
  borderRadius: "var(--hs-radius-sm)",
  padding: "10px 16px",
  cursor: "pointer",
  border: "1px solid var(--hs-border)",
};

const variants: Record<Variant, CSSProperties> = {
  primary: {
    background: "var(--hs-amber)",
    color: "var(--hs-on-amber)",
    borderColor: "transparent",
  },
  secondary: { background: "var(--hs-elevated)", color: "var(--hs-text)" },
  danger: { background: "var(--hs-live)", color: "#fff", borderColor: "var(--hs-live-border)" },
};

export function Button({
  variant = "secondary",
  style,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return <button type="button" style={{ ...base, ...variants[variant], ...style }} {...rest} />;
}

const pill: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 8,
  padding: "6px 12px",
  borderRadius: "var(--hs-radius-pill)",
  color: "#fff",
  fontWeight: 700,
  fontSize: 13,
  letterSpacing: 1,
};

export function LiveBadge({ children = "EN VIVO" }: { children?: ReactNode }) {
  return <span style={{ ...pill, background: "var(--hs-live)" }}>● {children}</span>;
}

export function PreviewBadge({ children = "PREVIEW" }: { children?: ReactNode }) {
  return <span style={{ ...pill, background: "var(--hs-preview)" }}>{children}</span>;
}

/** Brand mark from docs/design/Main.dc.html (amber rounded square with flame). */
export function Logo({ size = 32 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 40 40" aria-hidden="true">
      <rect x="1" y="1" width="38" height="38" rx="10" fill="#F2B544" />
      <path d="M20 9c3 4 5 6.5 5 10a5 5 0 0 1-10 0c0-3.5 2-6 5-10z" fill="#1A1405" />
      <path d="M12 30h16" stroke="#1A1405" strokeWidth="2.5" strokeLinecap="round" />
    </svg>
  );
}
