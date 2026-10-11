import type { Role } from "@hollyshow/protocol";
import { KEYS, safeStorage } from "../lib/safeStorage";

export interface Credentials {
  token: string;
  role: Role;
}

export type PairFailure = "invalid" | "locked" | "rate_limited" | "network" | "bad_request";

export class PairError extends Error {
  constructor(
    readonly failure: PairFailure,
    message: string,
  ) {
    super(message);
    this.name = "PairError";
  }
}

/** Where the page was opened: origin of the LAN server (also `vite dev` with `?host=`). */
export function serverOrigin(
  loc: Pick<Location, "protocol" | "host" | "search"> = location,
  allowOverride: boolean = import.meta.env.DEV,
): {
  http: string;
  ws: string;
} {
  // `?host=` is a dev-only convenience: in production it would let a crafted link send the stored
  // token or a typed PIN to an arbitrary server.
  const override = allowOverride ? new URLSearchParams(loc.search).get("host") : null;
  const host = override && /^[\w.\-:[\]]+$/.test(override) ? override : loc.host;
  const secure = loc.protocol === "https:";
  return {
    http: `${secure ? "https" : "http"}://${host}`,
    ws: `${secure ? "wss" : "ws"}://${host}/ws`,
  };
}

export interface LaunchParams {
  /** One-time QR token from `?pair=`. */
  pairToken: string | null;
  /** Open the public congregation view directly. */
  congregation: boolean;
}

export function readLaunchParams(search: string, hash: string): LaunchParams {
  const q = new URLSearchParams(search);
  const pair = q.get("pair");
  const congregation =
    q.get("view") === "congregation" ||
    q.has("c") ||
    /^#\/?(congregacion|congregation|c)\b/i.test(hash);
  return { pairToken: pair && /^[\w-]{8,256}$/.test(pair) ? pair : null, congregation };
}

/** Removes the one-time token from the address bar (it must not stay in history or be shared). */
export function stripPairParam(): void {
  try {
    const u = new URL(location.href);
    u.searchParams.delete("pair");
    history.replaceState(null, "", u.pathname + u.search + u.hash);
  } catch {
    /* ignore */
  }
}

export function loadCredentials(): Credentials | null {
  const token = safeStorage.get(KEYS.token);
  const role = safeStorage.get(KEYS.role) as Role | null;
  return token && role ? { token, role } : null;
}

export function saveCredentials(c: Credentials): void {
  safeStorage.set(KEYS.token, c.token);
  safeStorage.set(KEYS.role, c.role);
}

export function clearCredentials(): void {
  safeStorage.remove(KEYS.token);
  safeStorage.remove(KEYS.role);
}

export function defaultDeviceName(ua: string = navigator.userAgent): string {
  const saved = safeStorage.get(KEYS.deviceName);
  if (saved) return saved;
  const os = /iPhone|iPad/.test(ua)
    ? "iPhone"
    : /Android/.test(ua)
      ? "Android"
      : /Windows/.test(ua)
        ? "Windows"
        : /Mac/.test(ua)
          ? "Mac"
          : "Celular";
  return `${os} remoto`;
}

type FetchLike = (
  input: string,
  init?: RequestInit,
) => Promise<Pick<Response, "ok" | "status" | "json">>;

/** Redeems a PIN or QR token for a session token (`POST /api/pair`). */
export async function pair(
  http: string,
  cred: { pin: string } | { token: string },
  deviceName: string,
  fetchFn: FetchLike = (i, init) => fetch(i, init),
): Promise<Credentials> {
  let res: Awaited<ReturnType<FetchLike>>;
  try {
    res = await fetchFn(`${http}/api/pair`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ ...cred, deviceName }),
    });
  } catch {
    throw new PairError("network", "network");
  }
  if (res.ok) {
    const body = (await res.json()) as { token?: string; role?: Role };
    if (!body.token || !body.role) throw new PairError("bad_request", "bad response");
    return { token: body.token, role: body.role };
  }
  let message = "";
  try {
    message = ((await res.json()) as { message?: string }).message ?? "";
  } catch {
    /* body is optional */
  }
  if (res.status === 401) throw new PairError("invalid", message);
  if (res.status === 429)
    throw new PairError(/intent|lock|bloque/i.test(message) ? "locked" : "rate_limited", message);
  throw new PairError("bad_request", message);
}
