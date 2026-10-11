import { createMockApi, type HollyApi } from "@hollyshow/protocol";
import { createContext, useContext, useMemo, type ReactNode } from "react";
import { createTauriApi } from "./tauri";

const ApiContext = createContext<HollyApi | null>(null);

export const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Uses Tauri IPC inside the desktop shell and the in-memory mock in a plain browser (`pnpm dev:web`). */
export function ApiProvider({ children, api }: { children: ReactNode; api?: HollyApi }) {
  const value = useMemo(() => api ?? (inTauri() ? createTauriApi() : createMockApi()), [api]);
  return <ApiContext.Provider value={value}>{children}</ApiContext.Provider>;
}

export function useApi(): HollyApi {
  const api = useContext(ApiContext);
  if (!api) throw new Error("useApi must be used inside <ApiProvider>");
  return api;
}
