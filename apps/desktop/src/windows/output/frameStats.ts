/**
 * Frame pacing of the projector window, reported to the operator footer ("60 fps · 0 caídos").
 * Pure so it can be tested without a browser: feed it `requestAnimationFrame` timestamps.
 */
export class FrameStats {
  private last: number | null = null;
  private windowStart: number | null = null;
  private frames = 0;
  /** Frames that took longer than [`JANK_MS`] since the window opened. */
  dropped = 0;

  /** A frame slower than this (20 fps) counts as dropped. */
  static readonly JANK_MS = 50;

  /** Registers a frame; returns `{ fps, dropped }` once per `periodMs`, otherwise null. */
  tick(now: number, periodMs = 2000): { fps: number; dropped: number } | null {
    if (this.last !== null && now - this.last > FrameStats.JANK_MS) this.dropped++;
    this.last = now;
    this.windowStart ??= now;
    this.frames++;
    const elapsed = now - this.windowStart;
    if (elapsed < periodMs) return null;
    const out = { fps: (this.frames * 1000) / elapsed, dropped: this.dropped };
    this.windowStart = now;
    this.frames = 0;
    return out;
  }
}
