import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import type { Box } from "./layout";

/** `useLayoutEffect` on the client, `useEffect` on the server (no SSR warning in tests). */
export const useIsoLayoutEffect = typeof window === "undefined" ? useEffect : useLayoutEffect;

const DEFAULT_BOX: Box = { w: 1920, h: 1080 };

/** Tracks the pixel size of an element. A `fixed` box (thumbnails) skips observation. */
export function useBox(fixed?: Box): [RefObject<HTMLDivElement>, Box] {
  const ref = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState<Box>(fixed ?? DEFAULT_BOX);
  const fw = fixed?.w;
  const fh = fixed?.h;

  useIsoLayoutEffect(() => {
    if (fw !== undefined && fh !== undefined) {
      setBox({ w: fw, h: fh });
      return;
    }
    const el = ref.current;
    if (!el) return;
    const set = (w: number, h: number) =>
      setBox((prev) =>
        Math.abs(prev.w - w) < 0.5 && Math.abs(prev.h - h) < 0.5 ? prev : { w, h },
      );
    const r = el.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) set(r.width, r.height);
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(([e]) => {
      if (e && e.contentRect.width > 0) set(e.contentRect.width, e.contentRect.height);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [fw, fh]);

  return [ref, box];
}

/**
 * Increments when web fonts finish loading so text is re-measured with the real font instead of
 * the fallback (otherwise the first slide after launch could be fitted with the wrong metrics).
 */
export function useFontEpoch(): number {
  const [epoch, setEpoch] = useState(0);
  useEffect(() => {
    if (typeof document === "undefined" || !document.fonts) return;
    const bump = () => setEpoch((e) => e + 1);
    void document.fonts.ready.then(bump);
    document.fonts.addEventListener("loadingdone", bump);
    return () => document.fonts.removeEventListener("loadingdone", bump);
  }, []);
  return epoch;
}

export function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const q = window.matchMedia("(prefers-reduced-motion: reduce)");
    setReduced(q.matches);
    const on = (e: MediaQueryListEvent) => setReduced(e.matches);
    q.addEventListener("change", on);
    return () => q.removeEventListener("change", on);
  }, []);
  return reduced;
}
