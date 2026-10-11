import type { HollyApi } from "@hollyshow/protocol";
import { createContext, useContext, useMemo, type ReactNode } from "react";
import { createDemoBackend } from "./demo/backend";
import type { HostApi } from "./host";
import { createTauriApi } from "./tauri";
import { createTauriHost } from "./hostTauri";

const ApiContext = createContext<HollyApi | null>(null);
const HostContext = createContext<HostApi | null>(null);

export const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Uses Tauri IPC inside the desktop shell and an in-memory demo backend in a plain browser
 * (`pnpm dev:web`). Tests inject their own `api`/`host`.
 */
export function ApiProvider({
  children,
  api,
  host,
}: {
  children: ReactNode;
  api?: HollyApi;
  host?: HostApi;
}) {
  const value = useMemo(() => {
    if (api && host) return { api, host };
    if (inTauri()) return { api: api ?? createTauriApi(), host: host ?? createTauriHost() };
    const demo = createDemoBackend();
    return { api: api ?? demo.api, host: host ?? demo.host };
  }, [api, host]);
  return (
    <ApiContext.Provider value={value.api}>
      <HostContext.Provider value={value.host}>{children}</HostContext.Provider>
    </ApiContext.Provider>
  );
}

export function useApi(): HollyApi {
  const api = useContext(ApiContext);
  if (!api) throw new Error("useApi must be used inside <ApiProvider>");
  return api;
}

export function useHost(): HostApi {
  const host = useContext(HostContext);
  if (!host) throw new Error("useHost must be used inside <ApiProvider>");
  return host;
}
