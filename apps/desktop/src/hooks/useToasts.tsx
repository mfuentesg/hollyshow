import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

export interface Toast {
  id: number;
  kind: "info" | "error" | "success";
  text: string;
  /** Extra interactive content below the text (buttons, menus); gets a way to dismiss the toast. */
  content?: (dismiss: () => void) => ReactNode;
}

interface ToastApi {
  push: (text: string, kind?: Toast["kind"]) => void;
  error: (e: unknown, fallback?: string) => void;
  /** A toast with actions; stays `ttlMs` (default 15 s) so the operator has time to act. */
  action: (
    text: string,
    content: (dismiss: () => void) => ReactNode,
    kind?: Toast["kind"],
    ttlMs?: number,
  ) => void;
}

const ToastContext = createContext<ToastApi | null>(null);

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (
    e &&
    typeof e === "object" &&
    "message" in e &&
    typeof (e as { message: unknown }).message === "string"
  ) {
    return (e as { message: string }).message;
  }
  return "Error";
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const next = useRef(1);

  const show = useCallback((toast: Omit<Toast, "id">, ttlMs: number) => {
    const id = next.current++;
    setToasts((t) => [...t.slice(-3), { ...toast, id }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), ttlMs);
  }, []);
  const push = useCallback(
    (text: string, kind: Toast["kind"] = "info") =>
      show({ kind, text }, kind === "error" ? 7000 : 3500),
    [show],
  );
  const action = useCallback<ToastApi["action"]>(
    (text, content, kind = "info", ttlMs = 15000) => show({ kind, text, content }, ttlMs),
    [show],
  );
  const error = useCallback(
    (e: unknown, fallback?: string) => push(fallback ?? errorText(e), "error"),
    [push],
  );
  const api = useMemo(() => ({ push, error, action }), [push, error, action]);

  return (
    <ToastContext.Provider value={api}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        {toasts.map((t) => (
          <div
            key={t.id}
            className={`toast toast--${t.kind}`}
            role={t.kind === "error" ? "alert" : undefined}
          >
            {t.text}
            {t.content?.(() => setToasts((all) => all.filter((x) => x.id !== t.id)))}
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToasts(): ToastApi {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error("useToasts must be used inside <ToastProvider>");
  return ctx;
}
