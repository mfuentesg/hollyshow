import type { Event, Role, ServerMsg, ShowEntry, StateView, WireError } from "@hollyshow/protocol";

/**
 * Pure client-side protocol state: how server messages fold into what the UI shows, and when the
 * client must ask for a fresh snapshot. No I/O, so it is unit-tested without a socket.
 */
export interface SessionState {
  /** `connecting`: socket not yet welcomed. `syncing`: welcomed, waiting for the snapshot. */
  phase: "offline" | "connecting" | "syncing" | "ready";
  role: Role | null;
  serverName: string;
  protocol: number;
  heartbeatMs: number;
  capabilities: string[];
  /** Replacement session token the server issued (stored by the transport). */
  token: string | null;
  view: StateView | null;
  /** Expanded service; empty for roles that do not receive it. */
  show: ShowEntry[];
  hasShow: boolean;
  /** `server wall clock - local clock` (ms): add it to `Date.now()` to read the host clock. */
  clockOffset: number;
  /** A resync was requested and the snapshot has not arrived yet. */
  resyncPending: boolean;
  lastError: WireError | null;
}

export const initialSession = (): SessionState => ({
  phase: "offline",
  role: null,
  serverName: "",
  protocol: 0,
  heartbeatMs: 5000,
  capabilities: [],
  token: null,
  view: null,
  show: [],
  hasShow: false,
  clockOffset: 0,
  resyncPending: false,
  lastError: null,
});

export type SessionEffect =
  | { type: "resync" }
  | { type: "update"; version: number; view: StateView; events: Event[] }
  | { type: "reject"; error: WireError }
  | { type: "welcome"; token: string | null };

export interface Reduction {
  state: SessionState;
  effects: SessionEffect[];
}

/** The socket started a new connection attempt: keep the last view so the UI does not flash empty. */
export function onConnecting(s: SessionState): SessionState {
  return { ...s, phase: "connecting", resyncPending: false };
}

export function onDisconnected(s: SessionState): SessionState {
  return { ...s, phase: "offline", resyncPending: false };
}

/**
 * Folds one server message into the state.
 *
 * Version rules (docs/ARCHITECTURE.md §3): every change bumps `version` by one and the server
 * sends a `state` for each. So a `state` whose version is not `last + 1` means we missed something
 * (resync), an old one is ignored, and a new `showRevision` means the service changed (resync).
 */
export function reduceServerMsg(s: SessionState, msg: ServerMsg, localNow: number): Reduction {
  switch (msg.type) {
    case "welcome":
      return {
        state: {
          ...s,
          phase: "syncing",
          role: msg.role,
          serverName: msg.serverName,
          protocol: msg.protocol,
          heartbeatMs: msg.heartbeatMs > 0 ? msg.heartbeatMs : 5000,
          capabilities: msg.capabilities,
          token: msg.token,
          clockOffset: msg.nowMs - localNow,
          lastError: null,
        },
        effects: [{ type: "welcome", token: msg.token }],
      };

    case "snapshot": {
      const show = msg.snapshot.show;
      const view = msg.snapshot.view;
      return {
        state: {
          ...s,
          phase: "ready",
          view,
          show: show ?? [],
          hasShow: show !== null,
          clockOffset: msg.nowMs - localNow,
          resyncPending: false,
        },
        effects: [{ type: "update", version: view.version, view, events: [] }],
      };
    }

    case "state": {
      // Before the first snapshot there is nothing to compare with: the snapshot is authoritative.
      if (s.phase !== "ready" || !s.view) return { state: s, effects: [] };
      const prev = s.view;
      const clockOffset = msg.nowMs - localNow;
      if (msg.view.version < prev.version) {
        // Late duplicate (the feed is created before the snapshot).
        return { state: s, effects: [] };
      }
      if (msg.view.version === prev.version) {
        return { state: { ...s, clockOffset }, effects: [] };
      }
      const gap = msg.view.version !== prev.version + 1;
      const showChanged = msg.view.showRevision !== prev.showRevision;
      const needResync = (gap || showChanged) && !s.resyncPending;
      return {
        state: {
          ...s,
          view: msg.view,
          clockOffset,
          resyncPending: s.resyncPending || needResync,
        },
        effects: [
          { type: "update", version: msg.view.version, view: msg.view, events: [] },
          ...(needResync ? [{ type: "resync" } as const] : []),
        ],
      };
    }

    case "event":
      if (!s.view) return { state: s, effects: [] };
      return {
        state: s,
        effects: [{ type: "update", version: msg.version, view: s.view, events: [msg.event] }],
      };

    case "resync": {
      if (s.resyncPending) return { state: s, effects: [] };
      return { state: { ...s, resyncPending: true }, effects: [{ type: "resync" }] };
    }

    case "reject":
      return {
        state: { ...s, lastError: msg.error },
        effects: [{ type: "reject", error: msg.error }],
      };

    // Handled by the transport (pending maps) or irrelevant to state.
    case "ack":
    case "queryResult":
    case "pong":
    case "unknown":
      return { state: s, effects: [] };
  }
}

/** Rejections after which retrying cannot succeed without the person doing something. */
const TERMINAL = new Set([
  "invalid_token",
  "invalid_pin",
  "unauthorized",
  "kicked",
  "protocol_mismatch",
  "locked",
]);

export function isTerminalReject(code: string): boolean {
  return TERMINAL.has(code);
}
