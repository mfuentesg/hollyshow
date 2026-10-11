import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  DEFAULT_UPDATE_SETTINGS,
  ERR_BUSY,
  ERR_NOT_READY,
  type ReleaseInfo,
  type UpdateEvent,
  type UpdateSettingsValue,
  type UpdateStatus,
} from "./types";

/** What the update UI needs from the host. */
export interface UpdateApi {
  status(): Promise<UpdateStatus>;
  /** Manual check ("Buscar actualizaciones…"); also downloads what it finds. */
  check(): Promise<UpdateStatus>;
  /** Starts the download of an update found with auto download off. */
  download(): Promise<UpdateStatus>;
  /** Rejects with "busy" (service mode / live output) or "not_ready". */
  installNow(): Promise<void>;
  getSettings(): Promise<UpdateSettingsValue>;
  setSettings(s: UpdateSettingsValue): Promise<UpdateSettingsValue>;
  /** Subscribes to backend events; resolves to the unsubscribe function. */
  subscribe(handler: (e: UpdateEvent) => void): Promise<() => void>;
}

interface RawReady extends ReleaseInfo {
  deferred?: boolean;
}

export function createTauriUpdateApi(): UpdateApi {
  return {
    status: () => invoke<UpdateStatus>("update_status"),
    check: () => invoke<UpdateStatus>("update_check"),
    download: () => invoke<UpdateStatus>("update_download"),
    installNow: () => invoke<void>("update_install_now"),
    getSettings: () => invoke<UpdateSettingsValue>("update_settings_get"),
    setSettings: (settings) => invoke<UpdateSettingsValue>("update_settings_set", { settings }),
    async subscribe(handler) {
      const offs = await Promise.all([
        listen("update-checking", () => handler({ type: "checking" })),
        listen<ReleaseInfo>("update-available", (e) =>
          handler({ type: "available", info: e.payload }),
        ),
        listen<{ version: string; downloaded: number; total: number | null }>(
          "update-progress",
          (e) => handler({ type: "progress", ...e.payload }),
        ),
        listen<RawReady>("update-ready", (e) => {
          const { deferred, ...info } = e.payload;
          handler({ type: "ready", info, deferred: deferred ?? false });
        }),
        listen("update-none", () => handler({ type: "none" })),
        listen<{ message: string }>("update-error", (e) =>
          handler({ type: "error", message: e.payload.message }),
        ),
      ]);
      return () => offs.forEach((off) => off());
    },
  };
}

/**
 * In-memory implementation for `pnpm dev:web` and tests. `release` is what a check "finds";
 * `emit` lets a test push backend events.
 */
export function createMemoryUpdateApi(
  opts: {
    currentVersion?: string;
    release?: ReleaseInfo | null;
    busy?: boolean;
  } = {},
) {
  const handlers = new Set<(e: UpdateEvent) => void>();
  let settings = { ...DEFAULT_UPDATE_SETTINGS };
  let ready = false;
  const state = { busy: opts.busy ?? false, installed: false, checks: 0, downloads: 0 };
  const current = opts.currentVersion ?? "0.1.0";
  const emit = (e: UpdateEvent) => handlers.forEach((h) => h(e));

  const snapshot = (): UpdateStatus =>
    ready && opts.release
      ? {
          state: "ready",
          deferred: state.busy,
          ...opts.release,
          currentVersion: current,
          busy: state.busy,
        }
      : { state: "idle", currentVersion: current, busy: state.busy };

  const api: UpdateApi = {
    status: async () => snapshot(),
    async check() {
      state.checks++;
      emit({ type: "checking" });
      if (!opts.release) {
        emit({ type: "none" });
        return snapshot();
      }
      emit({ type: "available", info: opts.release });
      if (settings.autoDownload) await api.download();
      return snapshot();
    },
    async download() {
      state.downloads++;
      if (!opts.release) return snapshot();
      emit({ type: "progress", version: opts.release.version, downloaded: 50, total: 100 });
      ready = true;
      emit({ type: "ready", info: opts.release, deferred: state.busy });
      return snapshot();
    },
    async installNow() {
      if (!ready) throw ERR_NOT_READY;
      if (state.busy) throw ERR_BUSY;
      state.installed = true;
    },
    getSettings: async () => ({ ...settings }),
    async setSettings(s) {
      settings = { ...s };
      return { ...settings };
    },
    async subscribe(handler) {
      handlers.add(handler);
      return () => void handlers.delete(handler);
    },
  };
  return { api, emit, state };
}
