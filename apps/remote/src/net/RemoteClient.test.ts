import { PROTOCOL_VERSION, emptyState, type ClientMsg, type ServerMsg } from "@hollyshow/protocol";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RemoteClient, TransportError, type ClientEvent, type SocketLike } from "./RemoteClient";

class FakeSocket implements SocketLike {
  static all: FakeSocket[] = [];
  readyState = 0;
  sent: ClientMsg[] = [];
  onopen: SocketLike["onopen"] = null;
  onmessage: SocketLike["onmessage"] = null;
  onclose: SocketLike["onclose"] = null;
  onerror: SocketLike["onerror"] = null;
  constructor(readonly url: string) {
    FakeSocket.all.push(this);
  }
  send(data: string) {
    this.sent.push(JSON.parse(data) as ClientMsg);
  }
  close() {
    this.readyState = 3;
    this.onclose?.({});
  }
  // test helpers
  open() {
    this.readyState = 1;
    this.onopen?.({});
  }
  push(msg: ServerMsg) {
    this.onmessage?.({ data: JSON.stringify(msg) });
  }
  drop() {
    this.readyState = 3;
    this.onclose?.({});
  }
  get last(): ClientMsg | undefined {
    return this.sent[this.sent.length - 1];
  }
}

const view = (version: number, showRevision = 1) => {
  const { show: _s, ...v } = emptyState();
  return { ...v, version, showRevision };
};
const welcome = (over: Partial<Extract<ServerMsg, { type: "welcome" }>> = {}): ServerMsg => ({
  type: "welcome",
  protocol: PROTOCOL_VERSION,
  role: "operator",
  token: null,
  serverName: "iglesia-pc",
  heartbeatMs: 5000,
  capabilities: [],
  epoch: 1,
  nowMs: 1_000_000,
  ...over,
});
const snapshot = (version: number, showRevision = 1): ServerMsg => ({
  type: "snapshot",
  snapshot: { view: view(version, showRevision), show: [] },
  nowMs: 1_000_000,
});

function make(token: string | null = "tok") {
  FakeSocket.all = [];
  const client = new RemoteClient({
    url: "ws://x/ws",
    role: "operator",
    deviceName: "Pixel",
    token,
    createSocket: (u) => new FakeSocket(u),
    random: () => 0.5,
  });
  const events: ClientEvent[] = [];
  client.onClientEvent((e) => events.push(e));
  return { client, events, sock: () => FakeSocket.all[FakeSocket.all.length - 1]! };
}

function connectReady(c: ReturnType<typeof make>, version = 1) {
  c.client.connect();
  c.sock().open();
  c.sock().push(welcome());
  c.sock().push(snapshot(version));
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("RemoteClient handshake", () => {
  it("sends hello with the stored token, then becomes ready after the snapshot", () => {
    const c = make("tok");
    const statuses: string[] = [];
    c.client.onConnection((s) => statuses.push(s));
    c.client.connect();
    expect(c.client.getStatus()).toBe("connecting");
    c.sock().open();
    expect(c.sock().sent[0]).toMatchObject({
      type: "hello",
      protocolMin: PROTOCOL_VERSION,
      protocolMax: PROTOCOL_VERSION,
      role: "operator",
      token: "tok",
      pin: null,
    });
    c.sock().push(welcome());
    expect(c.client.getStatus()).toBe("open");
    expect(c.client.getSession().phase).toBe("syncing");
    c.sock().push(snapshot(4));
    expect(c.client.getSession().phase).toBe("ready");
    expect(statuses).toEqual(["connecting", "open"]);
  });

  it("emits a replacement token", () => {
    const c = make(null);
    c.client.connect();
    c.sock().open();
    c.sock().push(welcome({ token: "new-token" }));
    expect(c.events).toContainEqual({ type: "token", token: "new-token" });
  });

  it("getSnapshot resolves once the snapshot arrives", async () => {
    const c = make();
    c.client.connect();
    const p = c.client.getSnapshot();
    c.sock().open();
    c.sock().push(welcome());
    c.sock().push(snapshot(2));
    await expect(p).resolves.toMatchObject({ version: 2, show: [] });
  });

  it("estimates the host clock", () => {
    const c = make();
    vi.setSystemTime(990_000);
    connectReady(c);
    expect(c.client.serverNow()).toBe(1_000_000);
    vi.setSystemTime(991_000);
    expect(c.client.serverNow()).toBe(1_001_000);
  });
});

describe("commands and queries", () => {
  it("resolves a command with its ack by id", async () => {
    const c = make();
    connectReady(c);
    const p = c.client.dispatch({ kind: "blackout", on: true });
    const sent = c.sock().last as Extract<ClientMsg, { type: "command" }>;
    expect(sent.type).toBe("command");
    c.sock().push({
      type: "ack",
      id: sent.id,
      ack: { ok: true, version: 2, error: null, duplicate: false },
    });
    await expect(p).resolves.toMatchObject({ ok: true, version: 2 });
  });

  it("uses a distinct id per command", () => {
    const c = make();
    connectReady(c);
    void c.client.dispatch({ kind: "clear", on: true }).catch(() => undefined);
    void c.client.dispatch({ kind: "clear", on: false }).catch(() => undefined);
    const ids = c
      .sock()
      .sent.filter((m) => m.type === "command")
      .map((m) => (m as { id: string }).id);
    expect(new Set(ids).size).toBe(2);
  });

  it("resolves queries and rejects forbidden ones", async () => {
    const c = make();
    connectReady(c);
    const ok = c.client.query({ kind: "listBibleVersions" });
    const q = c.sock().last as Extract<ClientMsg, { type: "query" }>;
    c.sock().push({
      type: "queryResult",
      id: q.id,
      result: { kind: "bibleVersions", items: [] },
      error: null,
    });
    await expect(ok).resolves.toEqual({ kind: "bibleVersions", items: [] });

    const bad = c.client.query({ kind: "listPlaylists" });
    const q2 = c.sock().last as Extract<ClientMsg, { type: "query" }>;
    c.sock().push({
      type: "queryResult",
      id: q2.id,
      result: null,
      error: { code: "forbidden", message: "no" },
    });
    await expect(bad).rejects.toBeInstanceOf(TransportError);
  });

  it("rejects immediately while offline and times out unanswered requests", async () => {
    const c = make();
    await expect(c.client.dispatch({ kind: "clear", on: true })).rejects.toMatchObject({
      code: "disconnected",
    });
    connectReady(c);
    const p = c.client.dispatch({ kind: "clear", on: true });
    const assertion = expect(p).rejects.toMatchObject({ code: "timeout" });
    await vi.advanceTimersByTimeAsync(10_001);
    await assertion;
  });

  it("fails pending commands when the socket drops (never replays them)", async () => {
    const c = make();
    connectReady(c);
    const p = c.client.dispatch({ kind: "next", expectedVersion: 1, epoch: 1 });
    const assertion = expect(p).rejects.toMatchObject({ code: "disconnected" });
    c.sock().drop();
    await assertion;
    const before = FakeSocket.all.length;
    await vi.advanceTimersByTimeAsync(1_000);
    expect(FakeSocket.all.length).toBe(before + 1);
    const resent = FakeSocket.all.flatMap((s) => s.sent).filter((m) => m.type === "command");
    expect(resent).toHaveLength(1);
  });
});

describe("live updates", () => {
  it("notifies subscribers and exposes the new view", () => {
    const c = make();
    connectReady(c, 1);
    const seen: number[] = [];
    c.client.subscribe((u) => seen.push(u.version));
    c.sock().push({ type: "state", view: { ...view(2), blackout: true }, nowMs: 1 });
    expect(seen).toEqual([2]);
    expect(c.client.getSession().view?.blackout).toBe(true);
  });

  it("asks for a snapshot after a version gap and applies it", () => {
    const c = make();
    connectReady(c, 1);
    c.sock().push({ type: "state", view: view(5), nowMs: 1 });
    expect(c.sock().sent.filter((m) => m.type === "resync")).toHaveLength(1);
    c.sock().push(snapshot(5));
    expect(c.client.getSession().resyncPending).toBe(false);
  });

  it("asks for a snapshot when the show changes", () => {
    const c = make();
    connectReady(c, 1);
    c.sock().push({ type: "state", view: view(2, 2), nowMs: 1 });
    expect(c.sock().last).toEqual({ type: "resync" });
  });

  it("delivers events to subscribers", () => {
    const c = make();
    connectReady(c, 1);
    const kinds: string[] = [];
    c.client.subscribe((u) => u.events.forEach((e) => kinds.push(e.kind)));
    c.sock().push({ type: "event", version: 1, event: { kind: "libraryChanged", scope: "bible" } });
    expect(kinds).toEqual(["libraryChanged"]);
  });

  it("ignores garbage frames", () => {
    const c = make();
    connectReady(c);
    c.sock().onmessage?.({ data: "{not json" });
    c.sock().onmessage?.({ data: new ArrayBuffer(2) });
    expect(c.client.getSession().phase).toBe("ready");
  });
});

describe("reconnection", () => {
  it("reconnects with exponential backoff and resyncs through the new snapshot", async () => {
    const c = make();
    connectReady(c, 1);
    c.sock().drop();
    expect(c.client.getStatus()).toBe("reconnecting");
    expect(FakeSocket.all).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(500);
    expect(FakeSocket.all).toHaveLength(2);
    // The second attempt fails too: the next delay doubles.
    c.sock().open();
    c.sock().drop();
    await vi.advanceTimersByTimeAsync(999);
    expect(FakeSocket.all).toHaveLength(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(FakeSocket.all).toHaveLength(3);
    // Success resets the counter and replaces the state with the fresh snapshot.
    c.sock().open();
    c.sock().push(welcome());
    c.sock().push(snapshot(9, 3));
    expect(c.client.getStatus()).toBe("open");
    expect(c.client.getSession().view).toMatchObject({ version: 9, showRevision: 3 });
    c.sock().drop();
    await vi.advanceTimersByTimeAsync(500);
    expect(FakeSocket.all).toHaveLength(4);
  });

  it("nudge reconnects immediately instead of waiting for the backoff", async () => {
    const c = make();
    connectReady(c);
    c.sock().drop();
    c.client.nudge();
    expect(FakeSocket.all).toHaveLength(2);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(FakeSocket.all).toHaveLength(2);
  });

  it("does not reconnect after a terminal rejection", async () => {
    const c = make();
    c.client.connect();
    c.sock().open();
    c.sock().push({ type: "reject", error: { code: "invalid_token", message: "x" } });
    c.sock().drop();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(FakeSocket.all).toHaveLength(1);
    expect(c.events).toContainEqual({
      type: "rejected",
      error: { code: "invalid_token", message: "x" },
      terminal: true,
    });
    expect(c.client.getStatus()).toBe("closed");
  });

  it("keeps retrying after a transient rejection such as capacity", async () => {
    const c = make();
    c.client.connect();
    c.sock().open();
    c.sock().push({ type: "reject", error: { code: "capacity", message: "x" } });
    c.sock().drop();
    await vi.advanceTimersByTimeAsync(500);
    expect(FakeSocket.all).toHaveLength(2);
  });

  it("close stops everything", async () => {
    const c = make();
    connectReady(c);
    c.client.close();
    expect(c.client.getStatus()).toBe("closed");
    await vi.advanceTimersByTimeAsync(60_000);
    expect(FakeSocket.all).toHaveLength(1);
  });
});

describe("heartbeat", () => {
  it("pings every heartbeat and drops a silent connection", async () => {
    const c = make();
    connectReady(c);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(c.sock().sent.filter((m) => m.type === "ping")).toHaveLength(1);
    // Server answers once, then goes silent for more than three beats.
    c.sock().push({ type: "pong", t: 0 });
    await vi.advanceTimersByTimeAsync(5_000 * 4);
    expect(FakeSocket.all.length).toBeGreaterThanOrEqual(1);
    expect(c.client.getStatus()).toBe("reconnecting");
  });

  it("stays connected while the server keeps answering", async () => {
    const c = make();
    connectReady(c);
    for (let i = 0; i < 6; i++) {
      await vi.advanceTimersByTimeAsync(5_000);
      c.sock().push({ type: "pong", t: i });
    }
    expect(FakeSocket.all).toHaveLength(1);
    expect(c.client.getStatus()).toBe("open");
  });
});
