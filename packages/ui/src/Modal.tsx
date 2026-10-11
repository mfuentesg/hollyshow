import { useEffect, useId, useRef, type ReactNode } from "react";
import { IconClose } from "./icons";

const FOCUSABLE =
  'a[href],button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex]:not([tabindex="-1"])';

export interface ModalProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
  closeLabel?: string;
  /** Hides the title bar (command palette brings its own input). */
  bare?: boolean;
  align?: "top" | "center";
}

/** Accessible dialog: focus moves in, Tab is trapped, Esc closes, focus returns to the opener. */
export function Modal({
  title,
  onClose,
  children,
  footer,
  width = 560,
  closeLabel = "Close",
  bare,
  align = "top",
}: ModalProps) {
  const ref = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    const root = ref.current;
    if (root && !root.contains(document.activeElement)) {
      const first =
        root.querySelector<HTMLElement>("[data-autofocus]") ??
        root.querySelector<HTMLElement>(FOCUSABLE);
      (first ?? root).focus();
    }
    return () => opener?.focus?.();
  }, []);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onCloseRef.current();
      return;
    }
    if (e.key !== "Tab" || !ref.current) return;
    const items = Array.from(ref.current.querySelectorAll<HTMLElement>(FOCUSABLE));
    if (items.length === 0) {
      e.preventDefault();
      return;
    }
    const first = items[0]!;
    const last = items[items.length - 1]!;
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };

  return (
    <div
      className="hs-modal-backdrop"
      style={align === "center" ? { alignItems: "center", paddingTop: 16 } : undefined}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={ref}
        className="hs-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby={bare ? undefined : titleId}
        aria-label={bare ? title : undefined}
        tabIndex={-1}
        style={{ ["--hs-modal-width" as string]: `${width}px` }}
        onKeyDown={onKeyDown}
      >
        {!bare && (
          <div className="hs-modal__head">
            <h2 id={titleId}>{title}</h2>
            <button type="button" className="hs-iconbtn" aria-label={closeLabel} onClick={onClose}>
              <IconClose />
            </button>
          </div>
        )}
        <div className={bare ? undefined : "hs-modal__body"}>{children}</div>
        {footer && <div className="hs-modal__foot">{footer}</div>}
      </div>
    </div>
  );
}
