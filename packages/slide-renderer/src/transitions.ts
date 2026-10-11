import type { TransitionKind, TransitionSpec } from "@hollyshow/protocol";

/**
 * Transitions are described as plain keyframes touching only `opacity` and `transform`
 * (compositor-only, no layout or paint) and played with the Web Animations API. Building them is
 * pure so it is unit-tested; `playTransition` is the thin DOM part.
 */

export type Direction = "forward" | "back";

export interface TransitionPlan {
  /** Keyframes for the layer that leaves. `null` = leave untouched until removed. */
  outgoing: Keyframe[] | null;
  incoming: Keyframe[] | null;
  durationMs: number;
  easing: string;
  /** Delay applied to the incoming layer (the "fade through" style). */
  incomingDelayMs: number;
}

const EASE = "cubic-bezier(0.22, 0.61, 0.36, 1)";

export const cutPlan: TransitionPlan = {
  outgoing: null,
  incoming: null,
  durationMs: 0,
  easing: "linear",
  incomingDelayMs: 0,
};

/** Kinds that do not play on the DOM text layer (shader acts on the background). */
export function domKind(kind: TransitionKind): Exclude<TransitionKind, "shader"> {
  return kind === "shader" ? "crossfade" : kind;
}

export function buildTransition(
  spec: Pick<TransitionSpec, "kind" | "durationMs">,
  direction: Direction = "forward",
): TransitionPlan {
  const durationMs = Math.max(0, Math.min(5000, Math.round(spec.durationMs)));
  const kind = domKind(spec.kind);
  if (kind === "cut" || durationMs === 0) return cutPlan;
  const sign = direction === "forward" ? 1 : -1;
  switch (kind) {
    case "fade":
      // Fade through the background: the old text leaves first, then the new one arrives.
      return {
        outgoing: [{ opacity: 1 }, { opacity: 0 }],
        incoming: [{ opacity: 0 }, { opacity: 1 }],
        durationMs: Math.round(durationMs / 2),
        easing: "ease-in-out",
        incomingDelayMs: Math.round(durationMs / 2),
      };
    case "crossfade":
      return {
        outgoing: [{ opacity: 1 }, { opacity: 0 }],
        incoming: [{ opacity: 0 }, { opacity: 1 }],
        durationMs,
        easing: "ease-in-out",
        incomingDelayMs: 0,
      };
    case "slide":
      return {
        outgoing: [
          { opacity: 1, transform: "translate3d(0,0,0)" },
          { opacity: 0, transform: `translate3d(${-6 * sign}%,0,0)` },
        ],
        incoming: [
          { opacity: 0, transform: `translate3d(${6 * sign}%,0,0)` },
          { opacity: 1, transform: "translate3d(0,0,0)" },
        ],
        durationMs,
        easing: EASE,
        incomingDelayMs: 0,
      };
    case "zoom":
      return {
        outgoing: [
          { opacity: 1, transform: "scale(1)" },
          { opacity: 0, transform: "scale(1.12)" },
        ],
        incoming: [
          { opacity: 0, transform: "scale(0.9)" },
          { opacity: 1, transform: "scale(1)" },
        ],
        durationMs,
        easing: EASE,
        incomingDelayMs: 0,
      };
    case "blur":
      // Real blur needs `filter`, which is not compositor-only; the soft-focus feel is faked with
      // a slight scale + fade. The "shader" transition does a true blur dissolve on the background.
      return {
        outgoing: [
          { opacity: 1, transform: "scale(1)" },
          { opacity: 0, transform: "scale(1.04)" },
        ],
        incoming: [
          { opacity: 0, transform: "scale(0.97)" },
          { opacity: 1, transform: "scale(1)" },
        ],
        durationMs,
        easing: "ease-out",
        incomingDelayMs: 0,
      };
  }
}

/** Stagger delays for per-line reveal: lines arrive one after another within `totalMs`. */
export function lineRevealDelays(count: number, totalMs: number): number[] {
  if (count <= 0) return [];
  if (count === 1) return [0];
  const step = Math.min(350, Math.max(60, totalMs / count));
  return Array.from({ length: count }, (_, i) => Math.round(i * step));
}

export const lineRevealKeyframes: Keyframe[] = [
  { opacity: 0, transform: "translate3d(0,14px,0)" },
  { opacity: 1, transform: "translate3d(0,0,0)" },
];

export interface PlayOptions {
  plan: TransitionPlan;
  lineByLine: boolean;
  /** Elements to reveal in sequence in the incoming layer when `lineByLine`. */
  lines: Element[];
  reducedMotion?: boolean;
}

/** Plays the transition on the given elements. Resolves when everything finished or was cancelled. */
export function playTransition(
  outgoing: Element | null,
  incoming: Element | null,
  opts: PlayOptions,
): { finished: Promise<void>; cancel: () => void } {
  const anims: Animation[] = [];
  const { plan, lineByLine, lines } = opts;
  const animate = (el: Element | null, frames: Keyframe[] | null, delay: number, dur: number) => {
    if (!el || !frames || typeof el.animate !== "function") return;
    anims.push(el.animate(frames, { duration: dur, delay, easing: plan.easing, fill: "both" }));
  };
  if (!opts.reducedMotion) {
    if (lineByLine && lines.length > 0 && plan.durationMs > 0) {
      // Old text fades out quickly, new text appears line by line.
      animate(outgoing, [{ opacity: 1 }, { opacity: 0 }], 0, Math.min(200, plan.durationMs));
      const delays = lineRevealDelays(lines.length, plan.durationMs * 3);
      lines.forEach((el, i) => {
        if (typeof el.animate !== "function") return;
        anims.push(
          el.animate(lineRevealKeyframes, {
            duration: Math.max(180, plan.durationMs),
            delay: delays[i] ?? 0,
            easing: EASE,
            fill: "backwards",
          }),
        );
      });
    } else {
      animate(outgoing, plan.outgoing, 0, plan.durationMs);
      animate(incoming, plan.incoming, plan.incomingDelayMs, plan.durationMs);
    }
  }
  const finished = Promise.all(anims.map((a) => a.finished.catch(() => undefined))).then(
    () => undefined,
  );
  return {
    finished,
    cancel: () => anims.forEach((a) => a.cancel()),
  };
}
