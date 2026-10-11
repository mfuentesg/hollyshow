import {
  commands,
  type Command,
  type ItemId,
  type OutputId,
  type OutputLayers,
  type OutputLook,
  type PlaylistId,
  type Slide,
  type Timer,
  type TimerId,
} from "@hollyshow/protocol";

/**
 * Builders for commands that `@hollyshow/protocol`'s `commands` does not wrap yet (kept here to
 * respect package ownership; same wire shapes as `Command`).
 */
export const cmd = {
  ...commands,
  setActivePlaylist: (id: PlaylistId): Command => ({ kind: "setActivePlaylist", id }),
  refreshShow: (): Command => ({ kind: "refreshShow" }),
  updateSlide: (itemId: ItemId, slideIndex: number, slide: Slide): Command => ({
    kind: "updateSlide",
    itemId,
    slideIndex,
    slide,
  }),
  setTimer: (timer: Timer): Command => ({ kind: "setTimer", timer }),
  startTimer: (id: TimerId): Command => ({ kind: "startTimer", id }),
  pauseTimer: (id: TimerId): Command => ({ kind: "pauseTimer", id }),
  resetTimer: (id: TimerId): Command => ({ kind: "resetTimer", id }),
  removeTimer: (id: TimerId): Command => ({ kind: "removeTimer", id }),
  setOutputLook: (output: OutputId, look: OutputLook): Command => ({
    kind: "setOutputLook",
    output,
    look,
  }),
  setOutputLayers: (output: OutputId, layers: OutputLayers): Command => ({
    kind: "setOutputLayers",
    output,
    layers,
  }),
  removeOutput: (output: OutputId): Command => ({ kind: "removeOutput", output }),
  resetItemClock: (): Command => ({ kind: "resetItemClock" }),
} as const;
