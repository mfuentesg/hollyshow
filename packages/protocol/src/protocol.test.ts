import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { PROTOCOL_VERSION, commands, createMockApi } from "./index";

describe("protocol constants", () => {
  it("matches the Rust PROTOCOL_VERSION", () => {
    const rs = readFileSync(
      new URL("../../../crates/core/src/protocol.rs", import.meta.url),
      "utf8",
    );
    const m = /pub const PROTOCOL_VERSION: u32 = (\d+);/.exec(rs);
    expect(Number(m?.[1])).toBe(PROTOCOL_VERSION);
  });
});

describe("mock adapter", () => {
  it("walks the show and notifies subscribers", async () => {
    const api = createMockApi();
    const seen: number[] = [];
    api.subscribe((u) => seen.push(u.version));
    await api.dispatch(commands.next());
    await api.dispatch(commands.next());
    expect(api.state.live?.slide.id).toBe("item-1:1");
    expect(api.state.preview?.slide.id).toBe("item-1:2");
    expect(seen).toEqual([1, 2]);
  });

  it("rejects stale relative commands and keeps idempotent toggles quiet", async () => {
    const api = createMockApi();
    await api.dispatch(commands.next());
    const stale = await api.dispatch(commands.next(0));
    expect(stale.ok).toBe(false);
    expect(stale.error?.code).toBe("version_mismatch");
    const v = api.state.version;
    await api.dispatch(commands.blackout(true));
    await api.dispatch(commands.blackout(true));
    expect(api.state.version).toBe(v + 1);
  });

  it("answers typed queries", async () => {
    const api = createMockApi();
    const r = await api.query({ kind: "listThemes" });
    expect(r.kind).toBe("themes");
    expect(r.items).toEqual([]);
  });
});
