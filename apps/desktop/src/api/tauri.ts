import type {
  Ack,
  ApiUpdate,
  Command,
  HollyApi,
  PresentationState,
  QueryRequest,
  QueryResult,
  Update,
  WireError,
} from "@hollyshow/protocol";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Event emitted by the Rust shell after every accepted change. Keep in sync with src-tauri. */
export const UPDATE_EVENT = "hollyshow://update";

/** `HollyApi` over Tauri IPC (operator, output and stage windows). */
export function createTauriApi(): HollyApi {
  const listeners = new Set<(u: ApiUpdate) => void>();
  let unlisten: (() => void) | null = null;
  let closed = false;

  void listen<Update>(UPDATE_EVENT, (e) => {
    listeners.forEach((l) => l(e.payload));
  }).then((u) => {
    if (closed) u();
    else unlisten = u;
  });

  return {
    transport: "tauri",
    dispatch: (cmd: Command) => invoke<Ack>("dispatch", { id: crypto.randomUUID(), cmd }),
    query: async <Q extends QueryRequest>(query: Q) =>
      (await invoke("query", { query })) as QueryResult<Q>,
    getSnapshot: () => invoke<PresentationState>("get_snapshot"),
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    onConnection(listener) {
      listener("open");
      return () => {};
    },
    close() {
      closed = true;
      unlisten?.();
      listeners.clear();
    },
  };
}

export function isWireError(e: unknown): e is WireError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}
