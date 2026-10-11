import type { Slide, Theme } from "@hollyshow/protocol";
import { memo } from "react";
import { SlideView, type SlideViewProps } from "./SlideView";

export const THUMB_BOX = { w: 1920, h: 1080 } as const;

export interface SlideThumbnailProps extends Pick<
  SlideViewProps,
  "siblings" | "resolveMedia" | "position" | "background" | "outline"
> {
  slide: Slide | null;
  theme: Theme;
  /** Rendered width in CSS px; height follows 16:9. */
  width: number;
}

/**
 * Thumbnail: lays the slide out at a fixed 1920x1080 (so it fits exactly like the projector, with
 * one cached fit per slide) and scales it down with a transform. No video, transitions or timers.
 */
export const SlideThumbnail = memo(function SlideThumbnail({
  slide,
  theme,
  width,
  ...rest
}: SlideThumbnailProps) {
  const scale = width / THUMB_BOX.w;
  return (
    <div
      style={{
        width,
        height: THUMB_BOX.h * scale,
        overflow: "hidden",
        position: "relative",
        borderRadius: 6,
      }}
    >
      <div
        style={{
          width: THUMB_BOX.w,
          height: THUMB_BOX.h,
          transform: `scale(${scale})`,
          transformOrigin: "top left",
        }}
      >
        <SlideView slide={slide} theme={theme} mode="thumbnail" fixedBox={THUMB_BOX} {...rest} />
      </div>
    </div>
  );
});
