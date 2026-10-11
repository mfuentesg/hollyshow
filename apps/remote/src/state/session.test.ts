import { emptyState, type RoleSnapshot, type ServerMsg, type StateView } from "@hollyshow/protocol";
import { describe, expect, it } from "vitest";
import {
  initialSession,
  isTerminalReject,
  onDisconnected,
  reduceServerMsg,
  type SessionState,
} from "./session";

const view = (version: number, showRevision = 1): StateView => {
  const { show: _s, ...v } = emptyState();
  return { ...v, version, showRevision };
};

const welcome: ServerMsg = {
  type: "welcome",
  protocol: 1,
  role: "operator",
  token: "tok",
  serverName: "iglesia-pc",
  heartbeatMs: 5000,
  capabilities: [],
  nowMs: 10_000,
};
const snapshot = (v: StateView, show: RoleSnapshot["show"] = []): ServerMsg => ({
  type: "snapshot",
  snapshot: { view: v, show },
  nowMs: 10_050,
});

function ready(version = 5): SessionState {
  let s = reduceServerMsg(initialSession(), welcome, 9_000).state;
  s = reduceServerMsg(s, snapshot(view(version)), 9_000).state;
  return s;
}

describe("reduceServerMsg", () => {
  it("welcome stores identity, token and the clock offset", () => {
    const r = reduceServerMsg(initialSession(), welcome, 9_000);
    expect(r.state).toMatchObject({
      phase: "syncing",
      role: "operator",
      serverName: "iglesia-pc",
      clockOffset: 1_000,
    });
    expect(r.effects).toEqual([{ type: "welcome", token: "tok" }]);
  });

  it("snapshot makes the session ready and notifies", () => {
    const r = reduceServerMsg(
      reduceServerMsg(initialSession(), welcome, 0).state,
      snapshot(view(3), [{ itemId: "i", title: "T", slides: [], durationSecs: null }]),
      0,
    );
    expect(r.state.phase).toBe("ready");
    expect(r.state.show).toHaveLength(1);
    expect(r.state.hasShow).toBe(true);
    expect(r.effects[0]).toMatchObject({ type: "update", version: 3 });
  });

  it("roles without the show get an empty list", () => {
    const s = reduceServerMsg(
      reduceServerMsg(initialSession(), welcome, 0).state,
      snapshot(view(1), null),
      0,
    ).state;
    expect(s.show).toEqual([]);
    expect(s.hasShow).toBe(false);
  });

  it("applies the next version without asking for a resync", () => {
    const r = reduceServerMsg(ready(5), { type: "state", view: view(6), nowMs: 10_100 }, 9_100);
    expect(r.state.view?.version).toBe(6);
    expect(r.effects.map((e) => e.type)).toEqual(["update"]);
  });

  it("ignores stale and duplicate versions", () => {
    const s = ready(5);
    expect(reduceServerMsg(s, { type: "state", view: view(4), nowMs: 0 }, 0).effects).toEqual([]);
    const dup = reduceServerMsg(s, { type: "state", view: view(5), nowMs: 0 }, 0);
    expect(dup.effects).toEqual([]);
    expect(dup.state.view?.version).toBe(5);
  });

  it("requests one resync on a version gap and shows the newest view meanwhile", () => {
    const first = reduceServerMsg(ready(5), { type: "state", view: view(8), nowMs: 0 }, 0);
    expect(first.state.view?.version).toBe(8);
    expect(first.state.resyncPending).toBe(true);
    expect(first.effects.map((e) => e.type)).toEqual(["update", "resync"]);
    // A second gap before the snapshot arrives does not spam resync requests.
    const second = reduceServerMsg(first.state, { type: "state", view: view(10), nowMs: 0 }, 0);
    expect(second.effects.map((e) => e.type)).toEqual(["update"]);
    // The snapshot clears the flag.
    const done = reduceServerMsg(second.state, snapshot(view(10)), 0);
    expect(done.state.resyncPending).toBe(false);
  });

  it("requests a resync when the show revision changes", () => {
    const r = reduceServerMsg(ready(5), { type: "state", view: view(6, 2), nowMs: 0 }, 0);
    expect(r.effects.map((e) => e.type)).toEqual(["update", "resync"]);
  });

  it("ignores state before the snapshot", () => {
    const syncing = reduceServerMsg(initialSession(), welcome, 0).state;
    const r = reduceServerMsg(syncing, { type: "state", view: view(9), nowMs: 0 }, 0);
    expect(r.state).toBe(syncing);
    expect(r.effects).toEqual([]);
  });

  it("lagged resync from the server asks for a snapshot once", () => {
    const a = reduceServerMsg(ready(), { type: "resync", reason: "lagged" }, 0);
    expect(a.effects).toEqual([{ type: "resync" }]);
    expect(reduceServerMsg(a.state, { type: "resync", reason: "lagged" }, 0).effects).toEqual([]);
  });

  it("forwards events with the current view", () => {
    const r = reduceServerMsg(
      ready(),
      { type: "event", version: 5, event: { kind: "libraryChanged", scope: "bible" } },
      0,
    );
    expect(r.effects[0]).toMatchObject({ type: "update", events: [{ kind: "libraryChanged" }] });
  });

  it("rejects are surfaced", () => {
    const r = reduceServerMsg(
      ready(),
      { type: "reject", error: { code: "kicked", message: "x" } },
      0,
    );
    expect(r.effects).toEqual([{ type: "reject", error: { code: "kicked", message: "x" } }]);
    expect(r.state.lastError?.code).toBe("kicked");
  });

  it("disconnect keeps the last view for a calm UI", () => {
    const s = onDisconnected(ready(7));
    expect(s.phase).toBe("offline");
    expect(s.view?.version).toBe(7);
  });

  it("classifies terminal rejections", () => {
    expect(isTerminalReject("invalid_token")).toBe(true);
    expect(isTerminalReject("capacity")).toBe(false);
    expect(isTerminalReject("rate_limited")).toBe(false);
  });
});
