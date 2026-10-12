import { isStaleCommandCode } from "@hollyshow/protocol";
import type { Ack, Command, ConnectionStatus } from "@hollyshow/protocol";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import { useT } from "../lib/i18n";
import { buzz } from "../net/device";
import { TransportError, type RemoteClient } from "../net/RemoteClient";
import type { SessionState } from "./session";

interface RemoteCtx {
  client: RemoteClient;
  session: SessionState;
  status: ConnectionStatus;
  /** Sends a command with haptics and error toasts. Resolves null when it could not be delivered. */
  send(cmd: Command): Promise<Ack | null>;
  toast(message: string): void;
}

const Ctx = createContext<RemoteCtx | null>(null);

export function useRemote(): RemoteCtx {
  const c = useContext(Ctx);
  if (!c) throw new Error("useRemote outside RemoteProvider");
  return c;
}

export function RemoteProvider({
  client,
  children,
}: {
  client: RemoteClient;
  children: ReactNode;
}) {
  const t = useT();
  const session = useSyncExternalStore(
    (l) => client.onState(l),
    () => client.getSession(),
  );
  const status = useSyncExternalStore(
    (l) => client.onState(l),
    () => client.getStatus(),
  );
  const [message, setMessage] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const toast = useCallback((m: string) => {
    setMessage(m);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setMessage(null), 3500);
  }, []);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const send = useCallback(
    async (cmd: Command): Promise<Ack | null> => {
      buzz(12);
      try {
        const ack = await client.dispatch(cmd);
        if (!ack.ok) {
          const code = ack.error?.code ?? "";
          // A stale relative command (double tap, retry, host restart) is already resolved by the
          // state refresh.
          if (!isStaleCommandCode(code)) {
            buzz([30, 40, 30]);
            toast(
              code === "forbidden"
                ? t("err.forbidden")
                : code === "locked"
                  ? t("err.locked")
                  : ack.error?.message || t("err.generic"),
            );
          }
        }
        return ack;
      } catch (e) {
        buzz([30, 40, 30]);
        toast(
          e instanceof TransportError && e.code === "disconnected"
            ? t("err.offline")
            : t("err.generic"),
        );
        return null;
      }
    },
    [client, toast, t],
  );

  const value = useMemo(
    () => ({ client, session, status, send, toast }),
    [client, session, status, send, toast],
  );

  return (
    <Ctx.Provider value={value}>
      {children}
      {message && (
        <div className="toast" role="status">
          {message}
        </div>
      )}
    </Ctx.Provider>
  );
}

/** Re-renders every `ms` with the current host-clock time (timers, clock). */
export function useServerNow(client: RemoteClient, ms = 1000): number {
  const [now, setNow] = useState(() => client.serverNow());
  useEffect(() => {
    setNow(client.serverNow());
    const id = setInterval(() => setNow(client.serverNow()), ms);
    return () => clearInterval(id);
  }, [client, ms]);
  return now;
}
