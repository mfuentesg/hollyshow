import { useCallback, useState } from "react";

function read<T>(key: string, fallback: T): T {
  try {
    const raw = window.localStorage.getItem(`hollyshow.${key}`);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

/** Per-viewer convenience persisted in localStorage (never required for correctness). */
export function usePref<T>(key: string, fallback: T): [T, (v: T) => void] {
  const [value, setValue] = useState<T>(() => read(key, fallback));
  const set = useCallback(
    (v: T) => {
      setValue(v);
      try {
        window.localStorage.setItem(`hollyshow.${key}`, JSON.stringify(v));
      } catch {
        /* storage unavailable: keep the in-memory value */
      }
    },
    [key],
  );
  return [value, set];
}
