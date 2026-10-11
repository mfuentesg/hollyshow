import type { MediaId } from "@hollyshow/protocol";
import { useEffect, useRef, type CSSProperties } from "react";
import { LoopVideo } from "./LoopVideo";
import type { MediaResolver } from "./media";

const fill: CSSProperties = {
  position: "absolute",
  inset: 0,
  width: "100%",
  height: "100%",
  objectFit: "contain",
};

/** Foreground image/video between the background and the text. Fades in when it appears. */
export function MediaLayer({
  id,
  resolve,
  live,
  fadeMs,
}: {
  id: MediaId;
  resolve: MediaResolver;
  live: boolean;
  fadeMs: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el || !live || fadeMs <= 0 || typeof el.animate !== "function") return;
    const a = el.animate([{ opacity: 0 }, { opacity: 1 }], {
      duration: fadeMs,
      easing: "ease-out",
    });
    return () => a.cancel();
  }, [id, live, fadeMs]);
  const m = resolve(id);
  return (
    <div ref={ref} style={{ position: "absolute", inset: 0 }} data-media={id}>
      {m.kind === "video" ? (
        live ? (
          <LoopVideo src={m.url} loop muted />
        ) : null
      ) : (
        <img src={m.url} alt="" decoding="async" draggable={false} style={fill} />
      )}
    </div>
  );
}

/** Free text drawn above everything: lobby notice / lower-third alert. */
export function OverlayBar({ text, boxH, fadeMs }: { text: string; boxH: number; fadeMs: number }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el || fadeMs <= 0 || typeof el.animate !== "function") return;
    const a = el.animate(
      [
        { opacity: 0, transform: "translate3d(0,16px,0)" },
        { opacity: 1, transform: "translate3d(0,0,0)" },
      ],
      { duration: fadeMs, easing: "ease-out" },
    );
    return () => a.cancel();
  }, [text, fadeMs]);
  const size = Math.max(14, boxH * 0.045);
  return (
    <div
      style={{
        position: "absolute",
        left: "6%",
        right: "6%",
        bottom: "7%",
        display: "flex",
        justifyContent: "center",
        pointerEvents: "none",
      }}
    >
      <div
        ref={ref}
        role="status"
        style={{
          maxWidth: "100%",
          padding: `${size * 0.4}px ${size * 0.9}px`,
          borderRadius: size * 0.35,
          background: "rgba(8,10,14,.78)",
          borderLeft: `${Math.max(3, size * 0.14)}px solid #F2B544`,
          color: "#fff",
          fontWeight: 800,
          fontSize: size,
          lineHeight: 1.2,
          textAlign: "center",
          overflowWrap: "anywhere",
        }}
      >
        {text}
      </div>
    </div>
  );
}
