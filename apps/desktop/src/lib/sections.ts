import type { ShowEntry, Slide } from "@hollyshow/protocol";

/**
 * Section key shown in the slide grid and used for keyboard jumps: V1 V2 (verse), C (chorus),
 * P (pre-chorus), U (bridge/puente), I (intro), F (final/outro), T (tag), N (interludio).
 * Derived from the slide label because `Slide` carries no section kind.
 */
export function sectionKey(label: string | null): string | null {
  if (!label) return null;
  const l = label.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase().trim();
  let m = /^(?:verso|verse|v|estrofa)\s*(\d+)/.exec(l);
  if (m) return `V${m[1]}`;
  if (/^(pre[\s-]?coro|pre[\s-]?chorus)/.test(l)) return "P";
  if (/^(coro|chorus|estribillo|refrain)/.test(l)) return "C";
  if (/^(puente|bridge)/.test(l)) return "U";
  if (/^(intro)/.test(l)) return "I";
  if (/^(final|outro|cierre|ending)/.test(l)) return "F";
  if (/^(tag)/.test(l)) return "T";
  if (/^(interludio|interlude|instrumental)/.test(l)) return "N";
  m = /^([a-z])\d*$/.exec(l);
  return m ? m[0].toUpperCase() : null;
}

/** Index of the first slide whose section key is `key` (case-insensitive), or -1. */
export function findSectionSlide(slides: Slide[], key: string): number {
  const k = key.toUpperCase();
  return slides.findIndex((s) => sectionKey(s.label) === k);
}

/** Letters that complete a section key by themselves; `V` waits for a digit. */
const SINGLE = new Set(["C", "P", "U", "I", "F", "T", "N"]);

export interface KeyBuffer {
  buffer: string;
  at: number;
}

export const KEY_BUFFER_MS = 900;

/**
 * Feeds a pressed key into the section-jump buffer. Returns the new buffer and, when a key
 * sequence is complete, the section key ("V1", "C", ...).
 */
export function feedSectionKey(
  prev: KeyBuffer,
  key: string,
  now: number,
): { next: KeyBuffer; complete: string | null } {
  const k = key.toUpperCase();
  const live = now - prev.at < KEY_BUFFER_MS ? prev.buffer : "";
  if (live === "V" && /^[0-9]$/.test(k))
    return { next: { buffer: "", at: now }, complete: `V${k}` };
  if (k === "V") return { next: { buffer: "V", at: now }, complete: null };
  if (live === "" && /^[0-9]$/.test(k)) return { next: { buffer: "", at: now }, complete: `V${k}` };
  if (SINGLE.has(k)) return { next: { buffer: "", at: now }, complete: k };
  return { next: { buffer: "", at: now }, complete: null };
}

/** Whether `key` could be the start of a section key (so the handler should swallow it). */
export function isSectionKeyChar(key: string): boolean {
  const k = key.toUpperCase();
  return k === "V" || SINGLE.has(k) || /^[0-9]$/.test(k);
}

/** Position of the live cue inside the show: [entryIndex, slideIndex] or null. */
export function livePosition(
  show: ShowEntry[],
  itemId: string | null,
  slideIndex: number,
): [number, number] | null {
  if (!itemId) return null;
  const ei = show.findIndex((e) => e.itemId === itemId);
  if (ei < 0) return null;
  return slideIndex < show[ei]!.slides.length ? [ei, slideIndex] : null;
}
