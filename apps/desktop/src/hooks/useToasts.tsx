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
}

interface ToastApi {
  push: (text: string, kind?: Toast["kind"]) => void;
  error: (e: unknown, fallback?: string) => void;
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

  const push = useCallback((text: string, kind: Toast["kind"] = "info") => {
    const id = next.current++;
    setToasts((t) => [...t.slice(-3), { id, kind, text }]);
    setTimeout(
      () => setToasts((t) => t.filter((x) => x.id !== id)),
      kind === "error" ? 7000 : 3500,
    );
  }, []);
  const error = useCallback(
    (e: unknown, fallback?: string) => push(fallback ?? errorText(e), "error"),
    [push],
  );
  const api = useMemo(() => ({ push, error }), [push, error]);

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
