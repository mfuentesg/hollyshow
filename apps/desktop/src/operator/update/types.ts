/**
 * Wire types of the updater module. Hand written mirror of `apps/desktop/src-tauri/src/updater.rs`
 * (that crate does not export through ts-rs; keep both in sync).
 */

export type UpdateChannel = "stable" | "beta";

export interface UpdateSettingsValue {
  autoCheck: boolean;
  autoDownload: boolean;
  channel: UpdateChannel;
}

export const DEFAULT_UPDATE_SETTINGS: UpdateSettingsValue = {
  autoCheck: true,
  autoDownload: true,
  channel: "stable",
};

export interface ReleaseInfo {
  version: string;
  /** Markdown of the release. Rendered without raw HTML. */
  notes: string;
  /** Publication date, epoch ms. */
  date: number | null;
}

export type UpdateState =
  | { state: "idle" }
  | { state: "checking" }
  | ({ state: "available" } & ReleaseInfo)
  | ({ state: "downloading"; downloaded: number; total: number | null } & Partial<ReleaseInfo> & {
        version: string;
      })
  | ({ state: "ready"; deferred: boolean } & ReleaseInfo)
  | { state: "error"; message: string };

export type UpdateStatus = UpdateState & { currentVersion: string; busy: boolean };

/** Events emitted by the backend (`update-*`). */
export type UpdateEvent =
  | { type: "checking" }
  | { type: "available"; info: ReleaseInfo }
  | { type: "progress"; version: string; downloaded: number; total: number | null }
  | { type: "ready"; info: ReleaseInfo; deferred: boolean }
  | { type: "none" }
  | { type: "error"; message: string };

/** Command error codes returned by `update_install_now`. */
export const ERR_BUSY = "busy";
export const ERR_NOT_READY = "not_ready";
