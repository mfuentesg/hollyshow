import type { Command, ItemId, Passage, StateView, Target, VersionCode } from "./generated";

/**
 * What a relative command is issued against: the `(epoch, version)` of the last state seen. The
 * epoch identifies the host run, because `version` restarts at 0 whenever the app restarts.
 */
export type SeenState = Pick<StateView, "epoch" | "version">;

/** Small typed builders so call sites stay terse and typo-proof. */
export const commands = {
  next: (seen: SeenState | null = null): Command => ({
    kind: "next",
    expectedVersion: seen?.version ?? null,
    epoch: seen?.epoch ?? null,
  }),
  prev: (seen: SeenState | null = null): Command => ({
    kind: "prev",
    expectedVersion: seen?.version ?? null,
    epoch: seen?.epoch ?? null,
  }),
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
  ): Command => ({ kind: "showBible", passage, versions, layout, target }),
  stageMessage: (text: string | null): Command => ({ kind: "stageMessage", text }),
  serviceMode: (on: boolean): Command => ({ kind: "serviceMode", on }),
} as const;
