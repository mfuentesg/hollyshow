import type {
  Cue,
  OutputId,
  OutputKind,
  OutputLayers,
  ShowEntry,
  Slide,
  StateView,
  ThemeId,
} from "@hollyshow/protocol";
import { sectionKey, type Direction } from "@hollyshow/slide-renderer";

/** What one output draws (TS port of `StateView::frame_for` in crates/core/src/state.rs). */
export interface OutputFrame {
  output: OutputId;
  kind: OutputKind;
  themeId: ThemeId | null;
  blackout: boolean;
  logo: boolean;
  background: string | null;
  media: string | null;
  showText: boolean;
  overlay: string | null;
  cue: Cue | null;
}

const DEFAULT_LAYERS: OutputLayers = {
  background: null,
  media: null,
  text: true,
  overlay: null,
  blackout: false,
};

/** `null` for unknown or disabled outputs (the window then stays dark). */
export function frameFor(view: StateView, output: OutputId): OutputFrame | null {
  const look = view.outputs[output];
  if (!look?.enabled) return null;
  const layers = view.outputLayers[output] ?? DEFAULT_LAYERS;
  const blackout = (view.blackout && look.followsBlackout) || layers.blackout;
  return {
    output,
    kind: look.kind,
    themeId: look.themeId,
    blackout,
    logo: view.logo && !blackout,
    background: layers.background,
    media: layers.media,
    showText: layers.text && !view.clear && !blackout,
    overlay: blackout ? null : layers.overlay,
    cue: view.live,
  };
}

/** The slide that follows the live one in show order (crossing item boundaries), if any. */
export function nextSlideAfter(show: ShowEntry[], cue: Cue | null): Slide | null {
  if (!cue?.itemId) return null;
  const i = show.findIndex((e) => e.itemId === cue.itemId);
  const entry = show[i];
  if (!entry) return null;
  const within = entry.slides[cue.slideIndex + 1];
  if (within) return within;
  return show[i + 1]?.slides[0] ?? null;
}

/** Position used for the slide direction: absolute index across the show. */
export function absoluteIndex(show: ShowEntry[], cue: Cue | null): number | null {
  if (!cue?.itemId) return null;
  let n = 0;
  for (const e of show) {
    if (e.itemId === cue.itemId) return n + cue.slideIndex;
    n += e.slides.length;
  }
  return null;
}

export function directionBetween(prev: number | null, next: number | null): Direction {
  return prev !== null && next !== null && next < prev ? "back" : "forward";
}

/** Right-hand footer: "V1 · 2/3" (section key and position in the item). */
export function positionLabel(show: ShowEntry[], cue: Cue | null): string | undefined {
  if (!cue?.itemId) return undefined;
  const entry = show.find((e) => e.itemId === cue.itemId);
  if (!entry || entry.slides.length === 0) return undefined;
  const pos = `${cue.slideIndex + 1}/${entry.slides.length}`;
  const key = cue.slide.kind === "lyrics" ? sectionKey(cue.slide.label) : "";
  return key && key !== "·" ? `${key} · ${pos}` : pos;
}
