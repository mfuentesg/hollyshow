import type { ShowEntry } from "@hollyshow/protocol";
import type { OperatorCtx } from "./OperatorContext";

/** Entry shown in the slide grid: the selected one, else the live one, else the first with slides. */
export function selectedEntry(op: Pick<OperatorCtx, "p" | "selectedItemId">): ShowEntry | null {
  const { show, view } = op.p;
  return (
    show.find((e) => e.itemId === op.selectedItemId) ??
    show.find((e) => e.itemId === view.live?.itemId) ??
    show.find((e) => e.slides.length > 0) ??
    null
  );
}
