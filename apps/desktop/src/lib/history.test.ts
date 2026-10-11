import { describe, expect, it } from "vitest";
import { createHistory, pushHistory, redoHistory, undoHistory } from "./history";

describe("history", () => {
  it("undoes and redoes", () => {
    let h = createHistory("a");
    h = pushHistory(h, "b");
    h = pushHistory(h, "c");
    h = undoHistory(h);
    expect(h.present).toBe("b");
    h = undoHistory(h);
    expect(h.present).toBe("a");
    expect(undoHistory(h)).toBe(h);
    h = redoHistory(h);
    expect(h.present).toBe("b");
  });

  it("a new edit clears the redo stack", () => {
    let h = pushHistory(createHistory(1), 2);
    h = undoHistory(h);
    h = pushHistory(h, 3);
    expect(h.future).toEqual([]);
    expect(redoHistory(h)).toBe(h);
  });

  it("coalesces rapid edits with the same key into one step", () => {
    let h = createHistory("");
    h = pushHistory(h, "h", "text", 1000);
    h = pushHistory(h, "ho", "text", 1200);
    h = pushHistory(h, "hol", "text", 1300);
    expect(h.past).toEqual([""]);
    h = pushHistory(h, "hola", "text", 5000);
    expect(h.past).toEqual(["", "hol"]);
    h = pushHistory(h, "hola!", "other", 5100);
    expect(h.past.length).toBe(3);
  });

  it("ignores no-op pushes", () => {
    const h = createHistory({ a: 1 });
    expect(pushHistory(h, h.present)).toBe(h);
  });
});
