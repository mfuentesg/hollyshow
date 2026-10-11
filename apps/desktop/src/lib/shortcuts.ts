import { feedSectionKey, isSectionKeyChar, type KeyBuffer } from "./sections";

export type TargetKind = "text" | "dialog" | "tile" | "button" | "other";

export interface KeyInput {
  key: string;
  ctrl: boolean;
  meta: boolean;
  shift: boolean;
  alt: boolean;
  target: TargetKind;
}

export type ShortcutAction =
  | { type: "next" }
  | { type: "prev" }
  | { type: "takePreview" }
  | { type: "blackout" }
  | { type: "logo" }
  | { type: "clear" }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "palette" }
  | { type: "help" }
  | { type: "jump"; section: string; to: "live" | "preview" };

export interface ShortcutResult {
  action: ShortcutAction | null;
  buffer: KeyBuffer;
  /** Whether the browser default (scroll, button click...) must be suppressed. */
  consume: boolean;
}

/** Classifies the focused element so the pure handler needs no DOM. */
export function targetKind(el: EventTarget | null): TargetKind {
  if (!(el instanceof Element)) return "other";
  if (el.closest("[role=dialog]")) return "dialog";
  if (
    el.closest(
      "input:not([type=checkbox]):not([type=radio]):not([type=range]),textarea,select,[contenteditable=true]",
    )
  ) {
    return "text";
  }
  if (el.closest("[data-slide-tile]")) return "tile";
  if (el.closest("button,a,summary,[role=button],input")) return "button";
  return "other";
}

/**
 * Operator keyboard map (docs/design/Operator.dc.html footer):
 *  <- ->  Space  PageUp/PageDown   previous / next slide
 *  Enter                            preview -> live
 *  V1 V2 C P U I F T N              jump to a section (Shift = to preview)
 *  B  L  X                          black / logo / clear
 *  Ctrl+K  Ctrl+Z  Ctrl+Shift+Z     palette / undo / redo      ?  help
 */
export function decideKey(e: KeyInput, buffer: KeyBuffer, now: number): ShortcutResult {
  const none = (consume = false): ShortcutResult => ({ action: null, buffer, consume });
  const mod = e.ctrl || e.meta;
  const k = e.key;
  const lower = k.length === 1 ? k.toLowerCase() : k;

  if (mod && lower === "k") return { action: { type: "palette" }, buffer, consume: true };
  // Dialogs and text fields own the keyboard (native undo included), except the palette chord.
  if (e.target === "dialog" || e.target === "text") return none();

  if (mod && lower === "z")
    return { action: { type: e.shift ? "redo" : "undo" }, buffer, consume: true };
  if (mod && lower === "y") return { action: { type: "redo" }, buffer, consume: true };
  if (mod || e.alt) return none();

  // A focused button / link keeps only Enter and Space (its own activation); everything else
  // (arrows, B / L / X, section jumps, ?) must stay live after a click leaves focus on a button.
  const ownsActivation = e.target === "button" && (k === "Enter" || k === " ");
  if (ownsActivation) return none();
  switch (k) {
    case "ArrowRight":
    case "PageDown":
      return { action: { type: "next" }, buffer, consume: true };
    case "ArrowLeft":
    case "PageUp":
      return { action: { type: "prev" }, buffer, consume: true };
    case " ":
      return { action: { type: e.shift ? "prev" : "next" }, buffer, consume: true };
    case "Enter":
      return { action: { type: "takePreview" }, buffer, consume: true };
  }

  if (k === "?") return { action: { type: "help" }, buffer, consume: true };
  if (lower === "b") return { action: { type: "blackout" }, buffer, consume: true };
  if (lower === "l") return { action: { type: "logo" }, buffer, consume: true };
  if (lower === "x") return { action: { type: "clear" }, buffer, consume: true };

  if (k.length === 1 && isSectionKeyChar(k)) {
    const r = feedSectionKey(buffer, k, now);
    return {
      action: r.complete
        ? { type: "jump", section: r.complete, to: e.shift ? "preview" : "live" }
        : null,
      buffer: r.next,
      consume: true,
    };
  }
  return none();
}
