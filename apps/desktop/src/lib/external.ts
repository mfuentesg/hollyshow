import { openUrl } from "@tauri-apps/plugin-opener";
import { inTauri } from "../api/context";

/**
 * External links (release notes, image credits, licenses) always open in the system browser and
 * never navigate the webview. Only absolute https URLs are accepted; this mirrors the
 * `opener:allow-open-url` capability scope (`https://**`), so the UI never offers a link the
 * shell would refuse.
 */

/** Returns the normalized URL when `raw` is an absolute https URL with a host, else `null`. */
export function parseExternalUrl(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  let u: URL;
  try {
    u = new URL(raw.trim());
  } catch {
    return null;
  }
  if (u.protocol !== "https:" || !u.hostname || u.username || u.password) return null;
  return u.href;
}

export const isExternalUrl = (raw: unknown): boolean => parseExternalUrl(raw) !== null;

export type UrlOpener = (url: string) => Promise<void>;

/** Tauri: the opener plugin. Plain browser (demo, dev): a detached tab, never this one. */
const defaultOpener: UrlOpener = async (url) => {
  if (inTauri()) {
    await openUrl(url);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
};

/** Opens `raw` in the system browser. Resolves `false` (and opens nothing) for non-https input. */
export async function openExternal(
  raw: unknown,
  opener: UrlOpener = defaultOpener,
): Promise<boolean> {
  const url = parseExternalUrl(raw);
  if (!url) return false;
  try {
    await opener(url);
    return true;
  } catch (e) {
    console.warn("could not open external link", e);
    return false;
  }
}
