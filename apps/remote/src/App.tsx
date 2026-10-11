import type { Role } from "@hollyshow/protocol";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { KEYS, safeStorage } from "./lib/safeStorage";
import { keepAwake } from "./net/device";
import {
  clearCredentials,
  defaultDeviceName,
  loadCredentials,
  pair,
  readLaunchParams,
  saveCredentials,
  serverOrigin,
  stripPairParam,
  type Credentials,
} from "./net/pairing";
import { RemoteClient } from "./net/RemoteClient";
import { RemoteProvider, useRemote } from "./state/RemoteContext";
import { Congregation } from "./ui/Congregation";
import { Musician } from "./ui/Musician";
import { Operator } from "./ui/Operator";
import { PairScreen, errorKey } from "./ui/Pair";
import { Pastor } from "./ui/Pastor";
import { Header, OfflineBanner } from "./ui/common";
import type { MessageKey } from "./lib/i18n";

type Auth =
  | { kind: "booting" }
  | { kind: "pairing"; error: MessageKey | null }
  | { kind: "session"; creds: Credentials | null };

function useBoot(): {
  auth: Auth;
  deviceName: string;
  setDeviceName(n: string): void;
  pairWithPin(pin: string): Promise<void>;
  follow(): void;
  reset(error: MessageKey | null): void;
} {
  const launch = useMemo(() => readLaunchParams(location.search, location.hash), []);
  const [auth, setAuth] = useState<Auth>(() => {
    if (launch.pairToken) return { kind: "booting" };
    const creds = loadCredentials();
    if (creds) return { kind: "session", creds };
    if (launch.congregation || safeStorage.get(KEYS.mode) === "congregation") {
      return { kind: "session", creds: null };
    }
    return { kind: "pairing", error: null };
  });
  const [deviceName, setDeviceNameState] = useState(() => defaultDeviceName());
  const origin = useMemo(() => serverOrigin(), []);

  const setDeviceName = (n: string) => {
    setDeviceNameState(n);
    safeStorage.set(KEYS.deviceName, n);
  };

  // QR link: redeem the one-time token, then drop it from the address bar.
  const started = useRef(false);
  useEffect(() => {
    if (!launch.pairToken || started.current) return;
    started.current = true;
    pair(origin.http, { token: launch.pairToken }, defaultDeviceName())
      .then((creds) => {
        saveCredentials(creds);
        safeStorage.remove(KEYS.mode);
        setAuth({ kind: "session", creds });
      })
      .catch((e) => setAuth({ kind: "pairing", error: errorKey(e) }))
      .finally(stripPairParam);
  }, [launch.pairToken, origin.http]);

  const pairWithPin = async (pin: string) => {
    const creds = await pair(origin.http, { pin }, deviceName);
    saveCredentials(creds);
    safeStorage.remove(KEYS.mode);
    setAuth({ kind: "session", creds });
  };
  const follow = () => {
    safeStorage.set(KEYS.mode, "congregation");
    setAuth({ kind: "session", creds: null });
  };
  const reset = useCallback((error: MessageKey | null) => {
    clearCredentials();
    safeStorage.remove(KEYS.mode);
    setAuth({ kind: "pairing", error });
  }, []);
  return { auth, deviceName, setDeviceName, pairWithPin, follow, reset };
}

export function App() {
  const boot = useBoot();
  if (boot.auth.kind === "booting") return <div className="boot" aria-busy="true" />;
  if (boot.auth.kind === "pairing") {
    return (
      <PairScreen
        initialError={boot.auth.error}
        deviceName={boot.deviceName}
        onDeviceName={boot.setDeviceName}
        onPin={boot.pairWithPin}
        onFollow={boot.follow}
      />
    );
  }
  return <Connected creds={boot.auth.creds} deviceName={boot.deviceName} onRejected={boot.reset} />;
}

function Connected({
  creds,
  deviceName,
  onRejected,
}: {
  creds: Credentials | null;
  deviceName: string;
  onRejected(key: MessageKey): void;
}) {
  const client = useMemo(() => {
    const role: Role = creds?.role ?? "congregation";
    return new RemoteClient({
      url: serverOrigin().ws,
      role,
      deviceName,
      token: creds?.token ?? null,
    });
    // The device name only matters at pairing; reconnecting with a renamed device is not needed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [creds]);

  useEffect(() => {
    const offEvents = client.onClientEvent((e) => {
      if (e.type === "token") {
        saveCredentials({ token: e.token, role: creds?.role ?? "congregation" });
      } else if (e.type === "rejected" && e.terminal) {
        onRejected(e.error.code === "invalid_token" ? "pair.err.invalid" : "pair.err.unauthorized");
      }
    });
    client.connect();
    const wake = keepAwake();
    const nudge = () => client.nudge();
    const onVisible = () => document.visibilityState === "visible" && client.nudge();
    window.addEventListener("online", nudge);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      offEvents();
      wake();
      window.removeEventListener("online", nudge);
      document.removeEventListener("visibilitychange", onVisible);
      client.close();
    };
  }, [client, creds, onRejected]);

  return (
    <RemoteProvider client={client}>
      <RoleScreen fallback={creds?.role ?? "congregation"} />
    </RemoteProvider>
  );
}

function RoleScreen({ fallback }: { fallback: Role }) {
  const { session } = useRemote();
  switch (session.role ?? fallback) {
    case "operator":
      return <Operator />;
    case "pastor":
      return <Pastor />;
    case "musician":
      return <Musician />;
    case "congregation":
      return <Congregation />;
    case "stage":
      return <StageNote />;
  }
}

/** The stage role has its own display (served as an output page); on the phone it only shows state. */
function StageNote() {
  const { session, status } = useRemote();
  const live = session.view?.live?.slide.lines.join("\n") ?? "";
  return (
    <div className="screen">
      <Header
        title="HollyShow"
        status={status}
        serverName={session.serverName}
        role={session.role}
      />
      <OfflineBanner status={status} />
      <div className="screen-body">
        <div className="scroll">
          <p style={{ whiteSpace: "pre-line", fontSize: 24 }}>{live}</p>
        </div>
      </div>
    </div>
  );
}
