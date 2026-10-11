import {
  forwardRef,
  useId,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

const cx = (...parts: (string | false | null | undefined)[]) => parts.filter(Boolean).join(" ");

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: "md" | "sm";
  /** Large "panic" button (NEGRO / LIMPIAR / LOGO). */
  panic?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "md", panic, className, type = "button", ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      className={cx(
        "hs-btn",
        variant !== "secondary" && `hs-btn--${variant}`,
        size === "sm" && "hs-btn--sm",
        panic && "hs-btn--panic",
        className,
      )}
      {...rest}
    />
  );
});

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Required: icon-only controls need an accessible name. */
  "aria-label": string;
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { className, type = "button", ...rest },
  ref,
) {
  return <button ref={ref} type={type} className={cx("hs-iconbtn", className)} {...rest} />;
});

export interface ChipProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  pressed?: boolean;
  tone?: "default" | "blue" | "dashed";
}

/** Toggle pill (Bible version chips, arrangement chips). */
export const Chip = forwardRef<HTMLButtonElement, ChipProps>(function Chip(
  { pressed, tone = "default", className, type = "button", ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      aria-pressed={pressed}
      className={cx("hs-chip", tone !== "default" && `hs-chip--${tone}`, className)}
      {...rest}
    />
  );
});

export interface FieldProps {
  label: ReactNode;
  hint?: ReactNode;
  className?: string;
  style?: React.CSSProperties;
  children: (id: string, describedBy: string | undefined) => ReactNode;
}

/** Label + control with matching ids; the control comes from the render prop. */
export function Field({ label, hint, className, style, children }: FieldProps) {
  const id = useId();
  const hintId = hint ? `${id}-hint` : undefined;
  return (
    <div className={cx("hs-field", className)} style={style}>
      <label className="hs-label" htmlFor={id}>
        {label}
      </label>
      {children(id, hintId)}
      {hint && (
        <span id={hintId} className="hs-label">
          {hint}
        </span>
      )}
    </div>
  );
}

export const TextInput = forwardRef<
  HTMLInputElement,
  InputHTMLAttributes<HTMLInputElement> & { mono?: boolean }
>(function TextInput({ className, mono, ...rest }, ref) {
  return (
    <input ref={ref} className={cx("hs-input", mono && "hs-input--mono", className)} {...rest} />
  );
});

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(
  function Select({ className, ...rest }, ref) {
    return <select ref={ref} className={cx("hs-select", className)} {...rest} />;
  },
);

export const TextArea = forwardRef<
  HTMLTextAreaElement,
  TextareaHTMLAttributes<HTMLTextAreaElement>
>(function TextArea({ className, ...rest }, ref) {
  return <textarea ref={ref} className={cx("hs-textarea", className)} {...rest} />;
});

export interface SwitchRowProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  "type" | "onChange"
> {
  label: ReactNode;
  checked: boolean;
  onChange: (checked: boolean) => void;
}

/** Row with a 20 px native checkbox (keyboard and screen reader friendly), as in the mockups. */
export function SwitchRow({ label, checked, onChange, ...rest }: SwitchRowProps) {
  return (
    <label className="hs-switch">
      <span>{label}</span>
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        {...rest}
      />
    </label>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="hs-kbd">{children}</kbd>;
}

export function StatusDot({ state = "ok" }: { state?: "ok" | "off" | "warn" }) {
  return (
    <span
      className={cx("hs-dot", state === "off" && "hs-dot--off", state === "warn" && "hs-dot--warn")}
      aria-hidden="true"
    />
  );
}

export function Pill({
  children,
  tone,
  ...rest
}: { children: ReactNode; tone?: "amber" } & React.HTMLAttributes<HTMLSpanElement>) {
  return (
    <span className={cx("hs-pill", tone === "amber" && "hs-pill--amber")} {...rest}>
      {children}
    </span>
  );
}

export function LiveBadge({ children = "EN VIVO" }: { children?: ReactNode }) {
  return <span className="hs-badge hs-badge--live">● {children}</span>;
}

export function PreviewBadge({ children = "PREVIEW" }: { children?: ReactNode }) {
  return <span className="hs-badge hs-badge--preview">{children}</span>;
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
