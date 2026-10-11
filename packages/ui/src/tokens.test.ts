// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { tokens } from "./tokens";

describe("design tokens", () => {
  it("tokens.ts values appear in tokens.css", () => {
    const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8").toLowerCase();
    for (const v of Object.values(tokens)) expect(css).toContain(v.toLowerCase());
  });
});
