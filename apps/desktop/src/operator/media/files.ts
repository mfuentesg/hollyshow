import { convertFileSrc } from "@tauri-apps/api/core";

const REMOTE = /^(https?:|data:|blob:)/;

/** Turns a library file path into something an `<img>` can load (asset protocol inside Tauri). */
export function fileSrc(path: string): string {
  if (REMOTE.test(path)) return path;
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) return convertFileSrc(path);
  return path;
}

/** Offers `text` as a downloaded file (credits export). Returns false when the browser blocks it. */
export function saveTextFile(name: string, text: string, mime = "text/markdown"): boolean {
  try {
    const url = URL.createObjectURL(new Blob([text], { type: `${mime};charset=utf-8` }));
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    return true;
  } catch {
    return false;
  }
}
