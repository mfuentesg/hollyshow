import type { BibleVersion } from "@hollyshow/protocol";
import { usePref } from "../hooks/usePref";

/**
 * Versions shown on screen (max 4, order = column order). Persisted per viewer; defaults to the
 * default version followed by the secondary one. Codes no longer installed/enabled are dropped.
 */
export function defaultSelection(versions: BibleVersion[]): string[] {
  const def = versions.find((v) => v.isDefault) ?? versions[0];
  const sec = versions.find((v) => v.isSecondary && v.code !== def?.code);
  return [def?.code, sec?.code].filter((c): c is string => !!c);
}

export function useBibleSelection(versions: BibleVersion[]): [string[], (v: string[]) => void] {
  const [stored, setStored] = usePref<string[] | null>("bible.versions", null);
  const valid = (stored ?? defaultSelection(versions))
    .filter((c) => versions.some((v) => v.code === c))
    .slice(0, 4);
  const selection = valid.length > 0 || versions.length === 0 ? valid : defaultSelection(versions);
  return [selection, (v) => setStored(v.slice(0, 4))];
}
