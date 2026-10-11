import { afterEach, describe, expect, it } from "vitest";
import {
  SHADER_NAMES,
  SHADER_SOURCES,
  detectWebGL,
  pickShader,
  resetWebGLDetection,
} from "./webgl";

afterEach(() => resetWebGLDetection());

describe("webgl", () => {
  it("feature detection is false without a DOM (node) and memoised", () => {
    expect(detectWebGL()).toBe(false);
    expect(detectWebGL()).toBe(false);
  });
  it("ships three shaders sharing the transition uniforms", () => {
    expect(SHADER_NAMES).toHaveLength(3);
    for (const n of SHADER_NAMES) {
      const src = SHADER_SOURCES[n];
      expect(src).toContain("u_progress");
      expect(src).toContain("u_from");
      expect(src).toContain("u_to");
      expect(src).toContain("gl_FragColor");
    }
  });
  it("pickShader is deterministic and covers the set", () => {
    expect(pickShader("abc")).toBe(pickShader("abc"));
    const seen = new Set(Array.from({ length: 60 }, (_, i) => pickShader(`slide-${i}`)));
    expect(seen.size).toBe(3);
  });
});
