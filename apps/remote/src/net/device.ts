/** Phone niceties, all optional: every call degrades to a no-op when the API is missing. */

/** Short haptic tick (Android Chrome; iOS Safari has no Vibration API). */
export function buzz(pattern: number | number[] = 12): void {
  try {
    navigator.vibrate?.(pattern);
  } catch {
    /* blocked by the browser */
  }
}

interface WakeLockSentinelLike {
  release(): Promise<void>;
  addEventListener(type: "release", cb: () => void): void;
}

/**
 * Keeps the screen on while the page is visible. The lock is dropped by the browser whenever the
 * tab is hidden, so it is re-requested when the page becomes visible again.
 * Returns a function that releases it for good.
 */
export function keepAwake(): () => void {
  let sentinel: WakeLockSentinelLike | null = null;
  let stopped = false;
  const nav = navigator as Navigator & {
    wakeLock?: { request(type: "screen"): Promise<WakeLockSentinelLike> };
  };
  const acquire = async () => {
    if (stopped || !nav.wakeLock || document.visibilityState !== "visible" || sentinel) return;
    try {
      const s = await nav.wakeLock.request("screen");
      if (stopped) {
        void s.release();
        return;
      }
      sentinel = s;
      s.addEventListener("release", () => {
        if (sentinel === s) sentinel = null;
      });
    } catch {
      /* denied (battery saver, no user gesture yet): try again on the next visibility change */
    }
  };
  const onVisible = () => void acquire();
  document.addEventListener("visibilitychange", onVisible);
  document.addEventListener("pointerdown", onVisible, { once: true });
  void acquire();
  return () => {
    stopped = true;
    document.removeEventListener("visibilitychange", onVisible);
    document.removeEventListener("pointerdown", onVisible);
    void sentinel?.release().catch(() => undefined);
    sentinel = null;
  };
}
