import {
  PROTOCOL_VERSION,
  type Ack,
  type ApiUpdate,
  type ClientMsg,
  type Command,
  type ConnectionStatus,
  type HollyApi,
  type PresentationState,
  type QueryRequest,
  type QueryResult,
  type Role,
  type ServerMsg,
  type WireError,
} from "@hollyshow/protocol";
import { backoffDelay } from "../lib/backoff";
import {
  initialSession,
  isTerminalReject,
  onConnecting,
  onDisconnected,
  reduceServerMsg,
  type SessionEffect,
  type SessionState,
} from "../state/session";

/** The slice of the browser WebSocket the client uses (so tests can fake it). */
export interface SocketLike {
  readyState: number;
  send(data: string): void;
  close(code?: number, reason?: string): void;
  onopen: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onclose: ((ev: { code?: number }) => void) | null;
  onerror: ((ev: unknown) => void) | null;
}

export interface RemoteClientOptions {
  /** `ws://host/ws` */
  url: string;
  /** Role asked for in `hello` (the server uses the role stored with the pairing). */
  role: Role;
  deviceName: string;
  /** Session token from pairing; null for the public congregation view. */
  token: string | null;
  capabilities?: string[];
  createSocket?: (url: string) => SocketLike;
  now?: () => number;
  random?: () => number;
  /** Query/command timeout. */
  requestTimeoutMs?: number;
}

export type ClientEvent =
  { type: "token"; token: string } | { type: "rejected"; error: WireError; terminal: boolean };

const OPEN = 1;

interface Pending<T> {
  resolve: (v: T) => void;
  reject: (e: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

export class TransportError extends Error {
  constructor(
    readonly code: "disconnected" | "timeout" | "rejected" | "forbidden",
    message: string,
  ) {
    super(message);
    this.name = "TransportError";
  }
}

/**
 * `HollyApi` over the LAN server's WebSocket: hello -> welcome -> snapshot, role-filtered pushes,
 * heartbeat, exponential reconnection and snapshot resync on version gaps.
 *
 * Commands are never replayed after a reconnect (a retried "next" could skip a slide): their
 * promise rejects and the person taps again. Reads (`query`) are idempotent callers may retry.
 */
export class RemoteClient implements HollyApi {
  readonly transport = "websocket" as const;

  private session: SessionState = initialSession();
  private status: ConnectionStatus = "closed";
  private socket: SocketLike | null = null;
  private attempt = 0;
  private closed = true;
  private terminal = false;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private heartbeat: ReturnType<typeof setInterval> | null = null;
  private lastRx = 0;
  private seq = 0;
  private readonly salt: string;
  private token: string | null;

  private readonly acks = new Map<string, Pending<Ack>>();
  private readonly queries = new Map<string, Pending<unknown>>();
  private snapshotWaiters: Array<(s: PresentationState) => void> = [];

  private readonly updateListeners = new Set<(u: ApiUpdate) => void>();
  private readonly statusListeners = new Set<(s: ConnectionStatus) => void>();
  private readonly stateListeners = new Set<() => void>();
  private readonly eventListeners = new Set<(e: ClientEvent) => void>();

  private readonly createSocket: (url: string) => SocketLike;
  private readonly now: () => number;
  private readonly random: () => number;
  private readonly requestTimeoutMs: number;

  constructor(private readonly opts: RemoteClientOptions) {
    this.token = opts.token;
    this.now = opts.now ?? Date.now;
    this.random = opts.random ?? Math.random;
    this.requestTimeoutMs = opts.requestTimeoutMs ?? 10_000;
    this.createSocket = opts.createSocket ?? ((url) => new WebSocket(url) as unknown as SocketLike);
    this.salt = Math.floor(this.random() * 0xffffff)
      .toString(36)
      .padStart(4, "0");
  }

  // ---- lifecycle -------------------------------------------------------------------------

  connect(): void {
    if (!this.closed) return;
    this.closed = false;
    this.terminal = false;
    this.attempt = 0;
    this.open();
  }

  close(): void {
    this.closed = true;
    this.clearTimers();
    this.failPending(new TransportError("disconnected", "closed"));
    const s = this.socket;
    this.socket = null;
    if (s) {
      s.onopen = s.onmessage = s.onclose = s.onerror = null;
      try {
        s.close(1000);
      } catch {
        /* already closed */
      }
    }
    this.session = onDisconnected(this.session);
    this.setStatus("closed");
  }

  /** Reconnect now (page became visible, network came back) unless already healthy or finished. */
  nudge(): void {
    if (this.closed || this.terminal) return;
    if (this.socket && this.socket.readyState === OPEN && this.session.phase === "ready") {
      // Phones freeze sockets in the background without closing them: probe with a ping and
      // let the liveness check drop the socket if no answer arrives.
      this.lastRx = Math.min(this.lastRx, this.now() - this.session.heartbeatMs * 2);
      this.sendRaw({ type: "ping", t: this.now() });
      return;
    }
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
      this.attempt = 0;
      this.open();
    }
  }

  // ---- state access (for React's useSyncExternalStore) -----------------------------------------

  getSession(): SessionState {
    return this.session;
  }
  getStatus(): ConnectionStatus {
    return this.status;
  }
  /** Host clock estimate (epoch ms). */
  serverNow(): number {
    return this.now() + this.session.clockOffset;
  }
  onState(l: () => void): () => void {
    this.stateListeners.add(l);
    return () => this.stateListeners.delete(l);
  }
  onClientEvent(l: (e: ClientEvent) => void): () => void {
    this.eventListeners.add(l);
    return () => this.eventListeners.delete(l);
  }

  // ---- HollyApi ----------------------------------------------------------------------------

  dispatch(cmd: Command): Promise<Ack> {
    const id = `c-${this.salt}-${++this.seq}`;
    return this.request(this.acks, id, { type: "command", id, cmd });
  }

  async query<Q extends QueryRequest>(q: Q): Promise<QueryResult<Q>> {
    const id = `q-${this.salt}-${++this.seq}`;
    const res = await this.request(this.queries as Map<string, Pending<unknown>>, id, {
      type: "query",
      id,
      query: q,
    });
    return res as QueryResult<Q>;
  }

  getSnapshot(): Promise<PresentationState> {
    const s = this.session;
    if (s.phase === "ready" && s.view) {
      return Promise.resolve({ ...s.view, show: s.show });
    }
    return new Promise((resolve) => this.snapshotWaiters.push(resolve));
  }

  subscribe(listener: (u: ApiUpdate) => void): () => void {
    this.updateListeners.add(listener);
    return () => this.updateListeners.delete(listener);
  }

  onConnection(listener: (s: ConnectionStatus) => void): () => void {
    this.statusListeners.add(listener);
    return () => this.statusListeners.delete(listener);
  }

  // ---- internals -------------------------------------------------------------------------

  private request<T>(map: Map<string, Pending<T>>, id: string, msg: ClientMsg): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const s = this.socket;
      if (!s || s.readyState !== OPEN || this.session.phase !== "ready") {
        reject(new TransportError("disconnected", "sin conexion"));
        return;
      }
      const timer = setTimeout(() => {
        map.delete(id);
        reject(new TransportError("timeout", "tiempo de espera agotado"));
      }, this.requestTimeoutMs);
      map.set(id, { resolve, reject, timer });
      if (!this.sendRaw(msg)) {
        clearTimeout(timer);
        map.delete(id);
        reject(new TransportError("disconnected", "sin conexion"));
      }
    });
  }

  private sendRaw(msg: ClientMsg): boolean {
    const s = this.socket;
    if (!s || s.readyState !== OPEN) return false;
    try {
      s.send(JSON.stringify(msg));
      return true;
    } catch {
      return false;
    }
  }

  private open(): void {
    this.clearTimers();
    this.session = onConnecting(this.session);
    this.setStatus(
      this.attempt === 0 && this.session.view === null ? "connecting" : "reconnecting",
    );
    let ws: SocketLike;
    try {
      ws = this.createSocket(this.opts.url);
    } catch {
      this.scheduleReconnect();
      return;
    }
    this.socket = ws;
    ws.onopen = () => {
      if (this.socket !== ws) return;
      this.lastRx = this.now();
      this.sendRaw({
        type: "hello",
        protocolMin: PROTOCOL_VERSION,
        protocolMax: PROTOCOL_VERSION,
        role: this.opts.role,
        deviceName: this.opts.deviceName,
        token: this.token,
        pin: null,
        capabilities: this.opts.capabilities ?? ["bible-parallel", "timers"],
      });
    };
    ws.onmessage = (ev) => {
      if (this.socket !== ws || typeof ev.data !== "string") return;
      this.lastRx = this.now();
      let msg: ServerMsg;
      try {
        msg = JSON.parse(ev.data) as ServerMsg;
      } catch {
        return;
      }
      this.handle(msg);
    };
    ws.onerror = () => {
      /* the close event follows and drives the reconnect */
    };
    ws.onclose = () => {
      if (this.socket !== ws) return;
      this.socket = null;
      this.onSocketGone();
    };
  }

  private handle(msg: ServerMsg): void {
    switch (msg.type) {
      case "ack": {
        const p = this.acks.get(msg.id);
        if (p) {
          clearTimeout(p.timer);
          this.acks.delete(msg.id);
          p.resolve(msg.ack);
        }
        return;
      }
      case "queryResult": {
        const p = this.queries.get(msg.id);
        if (p) {
          clearTimeout(p.timer);
          this.queries.delete(msg.id);
          if (msg.result) p.resolve(msg.result);
          else
            p.reject(
              new TransportError("forbidden", msg.error?.message ?? msg.error?.code ?? "error"),
            );
        }
        return;
      }
      default:
        break;
    }
    const { state, effects } = reduceServerMsg(this.session, msg, this.now());
    const changed = state !== this.session;
    this.session = state;
    for (const e of effects) this.apply(e);
    if (changed) this.stateListeners.forEach((l) => l());
  }

  private apply(e: SessionEffect): void {
    switch (e.type) {
      case "welcome":
        if (e.token) {
          this.token = e.token;
          this.emit({ type: "token", token: e.token });
        }
        this.attempt = 0;
        this.startHeartbeat();
        this.setStatus("open");
        break;
      case "resync":
        this.sendRaw({ type: "resync" });
        break;
      case "update": {
        const u: ApiUpdate = { version: e.version, view: e.view, events: e.events };
        this.updateListeners.forEach((l) => l(u));
        if (
          this.session.phase === "ready" &&
          this.session.view &&
          this.snapshotWaiters.length > 0
        ) {
          const snap: PresentationState = { ...this.session.view, show: this.session.show };
          const waiters = this.snapshotWaiters;
          this.snapshotWaiters = [];
          waiters.forEach((w) => w(snap));
        }
        break;
      }
      case "reject": {
        const terminal = isTerminalReject(e.error.code);
        if (terminal) this.terminal = true;
        this.emit({ type: "rejected", error: e.error, terminal });
        break;
      }
    }
  }

  private emit(e: ClientEvent): void {
    this.eventListeners.forEach((l) => l(e));
  }

  private startHeartbeat(): void {
    if (this.heartbeat) clearInterval(this.heartbeat);
    const every = Math.max(1000, this.session.heartbeatMs);
    this.heartbeat = setInterval(() => {
      if (!this.socket) return;
      if (this.now() - this.lastRx > every * 3) {
        // Nothing (not even pongs) for three beats: the radio is gone. Drop and reconnect.
        const s = this.socket;
        this.socket = null;
        s.onopen = s.onmessage = s.onclose = s.onerror = null;
        try {
          s.close();
        } catch {
          /* ignore */
        }
        this.onSocketGone();
        return;
      }
      this.sendRaw({ type: "ping", t: this.now() });
    }, every);
  }

  private onSocketGone(): void {
    if (this.heartbeat) clearInterval(this.heartbeat);
    this.heartbeat = null;
    this.failPending(new TransportError("disconnected", "conexion perdida"));
    this.session = onDisconnected(this.session);
    if (this.closed || this.terminal) {
      this.setStatus("closed");
      this.stateListeners.forEach((l) => l());
      return;
    }
    this.stateListeners.forEach((l) => l());
    this.scheduleReconnect();
  }

  private scheduleReconnect(): void {
    this.setStatus("reconnecting");
    const delay = backoffDelay(this.attempt++, {}, this.random);
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (!this.closed && !this.terminal) this.open();
    }, delay);
  }

  private failPending(err: Error): void {
    for (const map of [this.acks, this.queries] as Array<Map<string, Pending<never>>>) {
      for (const p of map.values()) {
        clearTimeout(p.timer);
        p.reject(err);
      }
      map.clear();
    }
  }

  private clearTimers(): void {
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    if (this.heartbeat) clearInterval(this.heartbeat);
    this.reconnectTimer = null;
    this.heartbeat = null;
  }

  private setStatus(s: ConnectionStatus): void {
    if (s === this.status) return;
    this.status = s;
    this.statusListeners.forEach((l) => l(s));
    this.stateListeners.forEach((l) => l());
  }
}
