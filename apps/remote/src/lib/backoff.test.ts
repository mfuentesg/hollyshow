import { describe, expect, it } from "vitest";
import { backoffDelay } from "./backoff";

describe("backoffDelay", () => {
  const mid = () => 0.5; // no jitter

  it("doubles from 500 ms up to the cap", () => {
    expect([0, 1, 2, 3, 4, 5, 6, 10].map((n) => backoffDelay(n, {}, mid))).toEqual([
      500, 1000, 2000, 4000, 8000, 15000, 15000, 15000,
    ]);
  });

  it("keeps jitter within +-30%", () => {
    expect(backoffDelay(2, {}, () => 0)).toBe(1400);
    expect(backoffDelay(2, {}, () => 1)).toBe(2600);
  });

  it("survives absurd attempt counts", () => {
    expect(backoffDelay(10_000, {}, mid)).toBe(15000);
    expect(backoffDelay(-3, {}, mid)).toBe(500);
  });
});
