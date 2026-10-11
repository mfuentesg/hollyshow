import type { Command, ItemId, Passage, Target, VersionCode } from "./generated";

/** Small typed builders so call sites stay terse and typo-proof. */
export const commands = {
  next: (expectedVersion: number | null = null): Command => ({ kind: "next", expectedVersion }),
  prev: (expectedVersion: number | null = null): Command => ({ kind: "prev", expectedVersion }),
  goTo: (itemId: ItemId, slideIndex: number, target: Target = "live"): Command => ({
    kind: "goTo",
    itemId,
    slideIndex,
    target,
  }),
  showItem: (itemId: ItemId, target: Target = "live"): Command => ({
    kind: "showItem",
    itemId,
    target,
  }),
  takePreview: (): Command => ({ kind: "takePreview" }),
  blackout: (on: boolean): Command => ({ kind: "blackout", on }),
  clear: (on: boolean): Command => ({ kind: "clear", on }),
  logo: (on: boolean): Command => ({ kind: "logo", on }),
  showBible: (
    passage: Passage,
    versions: VersionCode[],
    target: Target = "live",
    layout: "columns" | "stacked" | "subtitle" = "columns",
  ): Command => ({ kind: "showBible", passage, versions, layout, target, resolved: null }),
  stageMessage: (text: string | null): Command => ({ kind: "stageMessage", text }),
  serviceMode: (on: boolean): Command => ({ kind: "serviceMode", on }),
} as const;
