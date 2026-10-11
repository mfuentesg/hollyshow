import { useEffect, useRef, type CSSProperties } from "react";

/** Seconds of overlap between the end of one copy and the start of the next. */
export const LOOP_OVERLAP_S = 0.8;

const fill: CSSProperties = {
  position: "absolute",
  inset: 0,
  width: "100%",
  height: "100%",
  objectFit: "cover",
};

/**
 * Seamless looping background video. Native `loop` shows a hitch (black frame / decoder reset) at
 * the seam; two stacked copies of the same file crossfade over the last `LOOP_OVERLAP_S` seconds
 * instead (double buffer). Short clips fall back to the native loop.
 */
export function LoopVideo({
  src,
  loop,
  muted,
  playing = true,
}: {
  src: string;
  loop: boolean;
  muted: boolean;
  playing?: boolean;
}) {
  const refs = [useRef<HTMLVideoElement>(null), useRef<HTMLVideoElement>(null)] as const;
  const active = useRef<0 | 1>(0);
  const swapping = useRef(false);

  useEffect(() => {
    const [a, b] = [refs[0].current, refs[1].current];
    if (!a || !b) return;
    active.current = 0;
    swapping.current = false;
    a.style.opacity = "1";
    b.style.opacity = "0";
    if (!playing) {
      a.pause();
      b.pause();
      return;
    }
    void a.play().catch(() => undefined);

    const onTime = () => {
      const cur = refs[active.current].current;
      const next = refs[active.current === 0 ? 1 : 0].current;
      if (!cur || !next || !loop || swapping.current) return;
      const d = cur.duration;
      if (!Number.isFinite(d) || d < LOOP_OVERLAP_S * 3) {
        cur.loop = true; // too short for a crossfaded seam
        return;
      }
      if (d - cur.currentTime > LOOP_OVERLAP_S) return;
      swapping.current = true;
      next.currentTime = 0;
      void next.play().catch(() => undefined);
      const anim =
        typeof next.animate === "function"
          ? next.animate([{ opacity: 0 }, { opacity: 1 }], {
              duration: LOOP_OVERLAP_S * 1000,
              fill: "forwards",
            })
          : null;
      const done = () => {
        next.style.opacity = "1";
        cur.style.opacity = "0";
        cur.pause();
        cur.currentTime = 0;
        active.current = active.current === 0 ? 1 : 0;
        swapping.current = false;
        anim?.cancel();
      };
      if (anim) anim.finished.then(done, () => (swapping.current = false));
      else done();
    };
    a.addEventListener("timeupdate", onTime);
    b.addEventListener("timeupdate", onTime);
    return () => {
      a.removeEventListener("timeupdate", onTime);
      b.removeEventListener("timeupdate", onTime);
      a.pause();
      b.pause();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [src, loop, playing]);

  return (
    <>
      <video ref={refs[0]} src={src} muted={muted} playsInline preload="auto" style={fill} />
      {loop && (
        <video
          ref={refs[1]}
          src={src}
          muted={muted}
          playsInline
          preload="auto"
          style={{ ...fill, opacity: 0 }}
        />
      )}
    </>
  );
}
