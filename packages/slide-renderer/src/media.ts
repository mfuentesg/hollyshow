import type { MediaId } from "@hollyshow/protocol";

export interface ResolvedMedia {
  url: string;
  kind: "image" | "video";
}

/** Maps an opaque media id to something the webview can load. Provided by the shell. */
export type MediaResolver = (id: MediaId) => ResolvedMedia;

const VIDEO_EXT = /\.(mp4|m4v|webm|mov|ogv)(\?|$)/i;

/**
 * Default resolver: assets are served by the shell's `hollyshow-media` protocol
 * (`hollyshow-media://localhost/<id>`, `http://hollyshow-media.localhost/<id>` on Windows).
 * The kind is guessed from the id's extension and defaults to image.
 */
export const defaultMediaResolver: MediaResolver = (id) => {
  const win = typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent);
  const base = win ? "http://hollyshow-media.localhost/" : "hollyshow-media://localhost/";
  return { url: base + encodeURIComponent(id), kind: VIDEO_EXT.test(id) ? "video" : "image" };
};

const warmed = new Set<string>();

/** Starts loading the media of a slide/background ahead of time (pre-render of the next slide). */
export function preloadMedia(
  ids: Array<MediaId | null | undefined>,
  resolve: MediaResolver = defaultMediaResolver,
): void {
  if (typeof Image === "undefined") return;
  for (const id of ids) {
    if (!id || warmed.has(id)) continue;
    warmed.add(id);
    const { url, kind } = resolve(id);
    if (kind === "image") {
      const img = new Image();
      img.decoding = "async";
      img.src = url;
    }
  }
}
