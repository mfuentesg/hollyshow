import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => cleanup());

// jsdom lacks these; components feature-detect but tests stay quiet.
if (typeof window !== "undefined") {
  window.HTMLElement.prototype.scrollIntoView ??= () => {};
  window.matchMedia ??= ((q: string) => ({
    matches: false,
    media: q,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    onchange: null,
    dispatchEvent: () => false,
  })) as typeof window.matchMedia;
}

// jsdom has no canvas: the fitter only needs a rough text width.
if (typeof HTMLCanvasElement !== "undefined") {
  HTMLCanvasElement.prototype.getContext = (() => ({
    font: "",
    measureText: (text: string) => ({ width: text.length * 10 }),
  })) as unknown as typeof HTMLCanvasElement.prototype.getContext;
}
