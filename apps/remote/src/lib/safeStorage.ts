/** localStorage that never throws (private windows, blocked site data, previews). */
export const safeStorage = {
  get(key: string): string | null {
    try {
      return globalThis.localStorage?.getItem(key) ?? null;
    } catch {
      return null;
    }
  },
  set(key: string, value: string): void {
    try {
      globalThis.localStorage?.setItem(key, value);
    } catch {
      /* storage unavailable: the app keeps working for this visit */
    }
  },
  remove(key: string): void {
    try {
      globalThis.localStorage?.removeItem(key);
    } catch {
      /* ignore */
    }
  },
  getJson<T>(key: string, fallback: T): T {
    const raw = this.get(key);
    if (raw === null) return fallback;
    try {
      return JSON.parse(raw) as T;
    } catch {
      return fallback;
    }
  },
  setJson(key: string, value: unknown): void {
    this.set(key, JSON.stringify(value));
  },
};

export const KEYS = {
  token: "hs.remote.token",
  role: "hs.remote.role",
  deviceName: "hs.remote.deviceName",
  mode: "hs.remote.mode",
  lang: "hs.remote.lang",
  crowd: "hs.remote.crowd",
  prayerOutbox: "hs.remote.prayerOutbox",
  bibleVersions: "hs.remote.bibleVersions",
  tab: "hs.remote.tab",
} as const;
