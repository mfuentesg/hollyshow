import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPrayers, readOutbox, submitPrayer } from "./prayer";

function memoryStorage() {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
  };
}

const ok = vi.fn().mockResolvedValue({ ok: true, status: 200 });
const notFound = vi.fn().mockResolvedValue({ ok: false, status: 404 });
const offline = vi.fn().mockRejectedValue(new Error("offline"));

beforeEach(() => {
  vi.stubGlobal("localStorage", memoryStorage());
  ok.mockClear();
});

describe("prayer requests", () => {
  it("posts and does not keep a copy on success", async () => {
    expect(await submitPrayer("http://h", { text: "  por mi familia ", name: " Ana " }, ok)).toBe(
      "sent",
    );
    expect(JSON.parse(ok.mock.calls[0]![1].body)).toEqual({ text: "por mi familia", name: "Ana" });
    expect(readOutbox()).toEqual([]);
  });

  it("queues when the server is unreachable or lacks the endpoint, then flushes in order", async () => {
    expect(await submitPrayer("http://h", { text: "uno", name: "" }, offline)).toBe("queued");
    expect(await submitPrayer("http://h", { text: "dos", name: "" }, notFound)).toBe("queued");
    expect(readOutbox().map((p) => p.text)).toEqual(["uno", "dos"]);

    expect(await flushPrayers("http://h", notFound)).toBe(0);
    expect(readOutbox()).toHaveLength(2);

    expect(await flushPrayers("http://h", ok)).toBe(2);
    expect(readOutbox()).toEqual([]);
    expect(ok.mock.calls.map((c) => JSON.parse(c[1].body).text)).toEqual(["uno", "dos"]);
  });

  it("survives corrupt storage", () => {
    localStorage.setItem("hs.remote.prayerOutbox", "{oops");
    expect(readOutbox()).toEqual([]);
    localStorage.setItem(
      "hs.remote.prayerOutbox",
      JSON.stringify([{ text: 1 }, { text: "a", name: "b" }]),
    );
    expect(readOutbox()).toEqual([{ text: "a", name: "b" }]);
  });
});
