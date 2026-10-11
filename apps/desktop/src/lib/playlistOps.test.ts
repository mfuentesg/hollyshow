import type { Playlist } from "@hollyshow/protocol";
import { describe, expect, it } from "vitest";
import {
  addItem,
  duplicateItem,
  instantiate,
  moveItem,
  newItem,
  removeItem,
  toTemplate,
  updateItem,
} from "./playlistOps";

const base = (): Playlist => ({
  id: "p",
  name: "Culto",
  date: "2026-10-11",
  isTemplate: false,
  items: ["a", "b", "c"].map((t) => ({ ...newItem(t, { kind: "header" }), id: t })),
});

describe("playlist ops", () => {
  it("moves items to a target index", () => {
    expect(moveItem(base(), 0, 2).items.map((i) => i.id)).toEqual(["b", "c", "a"]);
    expect(moveItem(base(), 2, 0).items.map((i) => i.id)).toEqual(["c", "a", "b"]);
    const p = base();
    expect(moveItem(p, 1, 1)).toBe(p);
    expect(moveItem(p, 1, 99).items.map((i) => i.id)).toEqual(["a", "c", "b"]);
  });

  it("adds, removes, updates and duplicates", () => {
    let p = addItem(base(), newItem("x", { kind: "header" }), 1);
    expect(p.items[1]!.title).toBe("x");
    p = removeItem(p, "a");
    expect(p.items.map((i) => i.id)).not.toContain("a");
    p = updateItem(p, "b", { durationSecs: 60, id: "hacked" });
    expect(p.items.find((i) => i.id === "b")!.durationSecs).toBe(60);
    const d = duplicateItem(p, "b");
    const idx = d.items.findIndex((i) => i.id === "b");
    expect(d.items[idx + 1]!.title).toBe("b");
    expect(d.items[idx + 1]!.id).not.toBe("b");
  });

  it("template and instantiate give fresh ids and never alias the source", () => {
    const t = toTemplate(base(), "Domingo AM");
    expect(t.isTemplate).toBe(true);
    expect(t.date).toBeNull();
    const s = instantiate(t, "Culto 18", "2026-10-18");
    expect(s.isTemplate).toBe(false);
    expect(s.items.map((i) => i.id)).not.toEqual(t.items.map((i) => i.id));
    expect(t.items.length).toBe(3);
  });
});
