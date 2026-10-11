import { describe, expect, it } from "vitest";
import { decideKey, type KeyInput } from "./shortcuts";

const key = (k: string, extra: Partial<KeyInput> = {}): KeyInput => ({
  key: k,
  ctrl: false,
  meta: false,
  shift: false,
  alt: false,
  target: "other",
  ...extra,
});
const empty = { buffer: "", at: 0 };

describe("operator shortcuts", () => {
  it("navigates with arrows, space and page keys", () => {
    expect(decideKey(key("ArrowRight"), empty, 1).action).toEqual({ type: "next" });
    expect(decideKey(key("PageUp"), empty, 1).action).toEqual({ type: "prev" });
    expect(decideKey(key(" "), empty, 1).action).toEqual({ type: "next" });
    expect(decideKey(key(" ", { shift: true }), empty, 1).action).toEqual({ type: "prev" });
  });

  it("Enter takes preview to live, also from a slide tile but not from a plain button", () => {
    expect(decideKey(key("Enter"), empty, 1).action).toEqual({ type: "takePreview" });
    expect(decideKey(key("Enter", { target: "tile" }), empty, 1).action).toEqual({
      type: "takePreview",
    });
    expect(decideKey(key("Enter", { target: "button" }), empty, 1).action).toBeNull();
  });

  it("B, L and X toggle black, logo and clear (case-insensitive)", () => {
    expect(decideKey(key("b"), empty, 1).action).toEqual({ type: "blackout" });
    expect(decideKey(key("B", { shift: true }), empty, 1).action).toEqual({ type: "blackout" });
    expect(decideKey(key("l"), empty, 1).action).toEqual({ type: "logo" });
    expect(decideKey(key("x"), empty, 1).action).toEqual({ type: "clear" });
  });

  it("never steals keys from text fields or dialogs, except Ctrl+K", () => {
    expect(decideKey(key("b", { target: "text" }), empty, 1).action).toBeNull();
    expect(decideKey(key("ArrowRight", { target: "dialog" }), empty, 1).action).toBeNull();
    expect(decideKey(key("k", { ctrl: true, target: "text" }), empty, 1).action).toEqual({
      type: "palette",
    });
    expect(decideKey(key("K", { meta: true }), empty, 1).action).toEqual({ type: "palette" });
  });

  it("section jumps: V then 1, C alone, Shift targets preview", () => {
    const a = decideKey(key("v"), empty, 100);
    expect(a.action).toBeNull();
    const b = decideKey(key("1"), a.buffer, 400);
    expect(b.action).toEqual({ type: "jump", section: "V1", to: "live" });
    expect(decideKey(key("C", { shift: true }), empty, 100).action).toEqual({
      type: "jump",
      section: "C",
      to: "preview",
    });
  });

  it("undo / redo chords", () => {
    expect(decideKey(key("z", { ctrl: true }), empty, 1).action).toEqual({ type: "undo" });
    expect(decideKey(key("z", { meta: true, shift: true }), empty, 1).action).toEqual({
      type: "redo",
    });
    expect(decideKey(key("y", { ctrl: true }), empty, 1).action).toEqual({ type: "redo" });
  });

  it("ignores unknown keys and alt chords", () => {
    expect(decideKey(key("q"), empty, 1).action).toBeNull();
    expect(decideKey(key("ArrowRight", { alt: true }), empty, 1).action).toBeNull();
  });

  it("panic, navigation and section keys still fire while a button holds focus", () => {
    const btn = { target: "button" } as const;
    expect(decideKey(key("b", btn), empty, 1).action).toEqual({ type: "blackout" });
    expect(decideKey(key("l", btn), empty, 1).action).toEqual({ type: "logo" });
    expect(decideKey(key("x", btn), empty, 1).action).toEqual({ type: "clear" });
    expect(decideKey(key("?", btn), empty, 1).action).toEqual({ type: "help" });
    expect(decideKey(key("ArrowRight", btn), empty, 1).action).toEqual({ type: "next" });
    expect(decideKey(key(" ", btn), empty, 1).action).toBeNull();
    const r1 = decideKey(key("v", btn), empty, 1);
    expect(decideKey(key("1", btn), r1.buffer, 2).action).toEqual({
      type: "jump",
      section: "V1",
      to: "live",
    });
  });
});
