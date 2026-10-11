import type { Background, MediaId, TransitionSpec } from "@hollyshow/protocol";
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { useIsoLayoutEffect } from "./hooks";
import { LoopVideo } from "./LoopVideo";
import { defaultMediaResolver, type MediaResolver } from "./media";
import { pickShader, ShaderTransition, detectWebGL } from "./webgl";

export interface BackgroundSpec {
  bg: Background;
  /** Per-output override (OutputLayers.background): a still image or video id. */
  overrideId: MediaId | null;
}

export function backgroundKey({ bg, overrideId }: BackgroundSpec): string {
  return overrideId ? `o:${overrideId}` : JSON.stringify(bg);
}

/** CSS `background` for the flat kinds. */
export function flatBackgroundCss(bg: Background): string {
  switch (bg.kind) {
    case "color":
      return bg.color;
    case "gradient":
      return `linear-gradient(${bg.angleDeg}deg, ${bg.from}, ${bg.to})`;
    default:
      return "transparent";
  }
}

const fill: CSSProperties = { position: "absolute", inset: 0, overflow: "hidden" };

function KenBurns({ src, blurPx, animate }: { src: string; blurPx: number; animate: boolean }) {
  const ref = useRef<HTMLImageElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el || !animate || typeof el.animate !== "function") return;
    const anim = el.animate(
      [
        { transform: "scale(1.02) translate3d(0,0,0)" },
        { transform: "scale(1.14) translate3d(-2%,-1.5%,0)" },
      ],
      { duration: 24000, direction: "alternate", iterations: Infinity, easing: "ease-in-out" },
    );
    return () => anim.cancel();
  }, [animate, src]);
  return (
    <img
      ref={ref}
      src={src}
      alt=""
      decoding="async"
      draggable={false}
      style={{
        ...fill,
        width: "100%",
        height: "100%",
        objectFit: "cover",
        filter: blurPx > 0 ? `blur(${blurPx}px)` : undefined,
        willChange: animate ? "transform" : undefined,
      }}
    />
  );
}

/** One background, no transition logic. */
export function BackgroundContent({
  spec,
  resolve,
  live,
  kenBurns = live,
}: {
  spec: BackgroundSpec;
  resolve: MediaResolver;
  /** false for thumbnails: no video, no Ken Burns. */
  live: boolean;
  /** false freezes the pan/zoom (the outgoing layer must not restart its animation). */
  kenBurns?: boolean;
}) {
  const { bg, overrideId } = spec;
  if (overrideId) {
    const m = resolve(overrideId);
    if (m.kind === "video") {
      return live ? (
        <LoopVideo src={m.url} loop muted />
      ) : (
        <div style={{ ...fill, background: "#000" }} />
      );
    }
    return <KenBurns src={m.url} blurPx={0} animate={false} />;
  }
  switch (bg.kind) {
    case "color":
    case "gradient":
      return <div style={{ ...fill, background: flatBackgroundCss(bg) }} />;
    case "transparent":
      return null;
    case "image": {
      const m = resolve(bg.mediaId);
      return <KenBurns src={m.url} blurPx={bg.blurPx} animate={kenBurns && bg.kenBurns} />;
    }
    case "video": {
      const m = resolve(bg.mediaId);
      return live ? (
        <LoopVideo src={m.url} loop={bg.loopPlayback} muted={bg.muted} />
      ) : (
        <div style={{ ...fill, background: "#000" }} />
      );
    }
  }
}

/** Rasterises flat/image backgrounds to a texture source for the WebGL path; null = unsupported. */
export async function rasterize(
  spec: BackgroundSpec,
  resolve: MediaResolver,
  w = 960,
  h = 540,
): Promise<TexImageSource | null> {
  const { bg, overrideId } = spec;
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d");
  if (!ctx) return null;
  const loadImage = (url: string) =>
    new Promise<HTMLImageElement>((ok, fail) => {
      const img = new Image();
      img.crossOrigin = "anonymous";
      img.onload = () => ok(img);
      img.onerror = () => fail(new Error("image"));
      img.src = url;
    });
  const drawCover = (img: HTMLImageElement) => {
    const s = Math.max(w / img.naturalWidth, h / img.naturalHeight);
    const dw = img.naturalWidth * s;
    const dh = img.naturalHeight * s;
    ctx.drawImage(img, (w - dw) / 2, (h - dh) / 2, dw, dh);
  };
  if (overrideId) {
    const m = resolve(overrideId);
    if (m.kind !== "image") return null;
    drawCover(await loadImage(m.url));
    return c;
  }
  switch (bg.kind) {
    case "color":
      ctx.fillStyle = bg.color;
      ctx.fillRect(0, 0, w, h);
      return c;
    case "gradient": {
      const a = ((bg.angleDeg - 90) * Math.PI) / 180;
      const len = Math.abs(w * Math.cos(a)) + Math.abs(h * Math.sin(a));
      const cx = w / 2;
      const cy = h / 2;
      const g = ctx.createLinearGradient(
        cx - (Math.cos(a) * len) / 2,
        cy - (Math.sin(a) * len) / 2,
        cx + (Math.cos(a) * len) / 2,
        cy + (Math.sin(a) * len) / 2,
      );
      g.addColorStop(0, bg.from);
      g.addColorStop(1, bg.to);
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, w, h);
      return c;
    }
    case "image":
      drawCover(await loadImage(resolve(bg.mediaId).url));
      return c;
    default:
      return null;
  }
}

/**
 * Background with change transitions. The old background stays on top and fades out over the new
 * one (no brightness dip, unlike a symmetric crossfade of two translucent layers). When the
 * theme transition is `shader` and WebGL works, a canvas plays a GLSL transition instead; any
 * failure falls back to the fade.
 */
export function BackgroundLayer({
  spec,
  resolve = defaultMediaResolver,
  transition,
  live = true,
  transitionKey,
}: {
  spec: BackgroundSpec;
  resolve?: MediaResolver;
  transition: Pick<TransitionSpec, "kind" | "durationMs">;
  live?: boolean;
  transitionKey?: string;
}) {
  const key = backgroundKey(spec);
  const [prev, setPrev] = useState<{ spec: BackgroundSpec; token: number } | null>(null);
  const lastRef = useRef<{ key: string; spec: BackgroundSpec }>({ key, spec });
  const tokenRef = useRef(0);
  const prevEl = useRef<HTMLDivElement>(null);
  const canvasEl = useRef<HTMLCanvasElement>(null);

  useIsoLayoutEffect(() => {
    const last = lastRef.current;
    lastRef.current = { key, spec };
    if (last.key !== key && live && transition.durationMs > 0 && transition.kind !== "cut") {
      setPrev({ spec: last.spec, token: ++tokenRef.current });
    } else if (last.key !== key) {
      setPrev(null);
    }
    // `spec` is derived from `key`; resolve identity changes must not retrigger.
  }, [key]);

  const prevToken = prev?.token;
  const prevSpec = prev?.spec;
  useEffect(() => {
    if (prevToken === undefined || !prevSpec) return;
    let cancelled = false;
    let shader: ShaderTransition | null = null;
    let anim: Animation | null = null;
    const canvasNode = canvasEl.current;
    const done = () => {
      if (!cancelled) setPrev((p) => (p?.token === prevToken ? null : p));
    };
    const fade = () => {
      const el = prevEl.current;
      if (!el || typeof el.animate !== "function") return done();
      anim = el.animate([{ opacity: 1 }, { opacity: 0 }], {
        duration: transition.durationMs,
        easing: "ease-in-out",
        fill: "forwards",
      });
      anim.finished.then(done, () => undefined);
    };
    const wantsShader = transition.kind === "shader" && detectWebGL() && canvasEl.current !== null;
    if (!wantsShader) {
      fade();
    } else {
      void (async () => {
        try {
          const [from, to] = await Promise.all([
            rasterize(prevSpec, resolve),
            rasterize(spec, resolve),
          ]);
          const canvas = canvasEl.current;
          if (cancelled) return;
          if (!from || !to || !canvas) return fade();
          canvas.width = 960;
          canvas.height = 540;
          shader = new ShaderTransition(canvas);
          canvas.style.display = "block";
          if (prevEl.current) prevEl.current.style.visibility = "hidden";
          await shader.run(
            pickShader(transitionKey ?? key),
            from,
            to,
            Math.max(300, transition.durationMs),
          );
          done();
        } catch {
          if (prevEl.current) prevEl.current.style.visibility = "visible";
          if (canvasEl.current) canvasEl.current.style.display = "none";
          if (!cancelled) fade();
        }
      })();
    }
    return () => {
      cancelled = true;
      anim?.cancel();
      shader?.dispose();
      if (canvasNode) canvasNode.style.display = "none";
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prevToken]);

  return (
    <div style={fill} aria-hidden="true">
      <BackgroundContent spec={spec} resolve={resolve} live={live} />
      {prevSpec && (
        <div ref={prevEl} style={fill}>
          <BackgroundContent spec={prevSpec} resolve={resolve} live={live} kenBurns={false} />
        </div>
      )}
      <canvas ref={canvasEl} style={{ ...fill, width: "100%", height: "100%", display: "none" }} />
    </div>
  );
}
