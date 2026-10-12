import type {
  BibleVersion,
  MonitorInfo,
  OutputId,
  Playlist,
  PlaylistId,
  Song,
  SongId,
  Theme,
  ThemeId,
  VersionCode,
} from "@hollyshow/protocol";

/**
 * Host operations the operator window needs beyond `HollyApi` (which covers show control and
 * read-only library queries): library writes, Bible installation, monitors, remote pairing and
 * app health. Implemented by `hostTauri.ts` (IPC command per method, named in `HOST_COMMANDS`) and
 * by `demo/` for `pnpm dev:web` and tests.
 *
 * After every library write the shell must call `AppService::library_changed` /
 * `playlist_saved` (ARCHITECTURE §3) so the show and the other clients refresh.
 */

export type RemoteRole = "operator" | "musician" | "pastor" | "stage" | "congregation";

export interface RemoteDevice {
  id: string;
  name: string;
  role: RemoteRole;
  /** Epoch ms of the pairing. */
  pairedAtMs: number;
  /** Epoch ms of the last message. */
  lastSeenMs: number;
  connected: boolean;
}

export interface PairingInfo {
  enabled: boolean;
  /** LAN URL phones open, e.g. `http://hollyshow.local:7420`. */
  url: string;
  /** Six digit PIN shown to the operator. */
  pin: string;
  /** Epoch ms when the PIN stops being valid (null = until regenerated). */
  pinExpiresMs: number | null;
  /** Same address carrying the one-time token: the QR encodes this so scanning pairs by itself. */
  qrUrl?: string;
  /** Every address phones could try, best first (the Wi-Fi adapter before virtual ones). */
  urls?: string[];
  /** False when the PC has no LAN address; the UI says so instead of drawing a useless QR. */
  networkAvailable?: boolean;
}

/** Roles a PIN can grant (the congregation joins without pairing). */
export type PairingRole = Exclude<RemoteRole, "congregation">;

/** What the host recovered when it started ("retomar donde estabas"). */
export interface StartupInfo {
  /** The previous service and/or what was on screen was restored. */
  resumed: boolean;
  /** The previous run ended without a clean exit (crash, power loss). */
  crashed: boolean;
  playlist: string | null;
  hadLive: boolean;
  /** The library file was damaged and had to be restored from a backup. */
  dbRecovered: boolean;
}

export interface HealthInfo {
  /** Output frames per second measured by the output window. */
  fps: number;
  droppedFrames: number;
  cpuPercent: number;
  ramMb: number;
}

export type SongImportFormat = "auto" | "openlyrics" | "chordpro" | "opensong" | "text";

export interface SongImportFile {
  name: string;
  content: string;
  format?: SongImportFormat;
}

export interface SongImportResult {
  imported: Song[];
  /** `[file name, reason]` for files that could not be parsed. */
  failed: [string, string][];
}

export type BibleImportFormat =
  "auto" | "hsb" | "osis" | "usfm" | "zefania" | "opensong" | "tabular";

export interface BibleImportRequest {
  fileName: string;
  /** File bytes, base64 encoded (IPC friendly). */
  dataBase64: string;
  format?: BibleImportFormat;
  /** Optional overrides for formats that carry no metadata. */
  code?: string;
  name?: string;
  language?: string;
  isPublicDomain?: boolean;
  license?: string;
}

export interface BibleCatalogEntry {
  code: VersionCode;
  name: string;
  language: string;
  license: string;
  isPublicDomain: boolean;
  /** Where the text comes from (documented for every bundled version). */
  source: string;
  installed: boolean;
}

export interface BibleVersionPatch {
  isEnabled?: boolean;
  isDefault?: boolean;
  isSecondary?: boolean;
}

/** One row of the CCLI report: a song and how often it was shown in the period. */
export interface UsageRow {
  songId: string;
  title: string | null;
  author: string | null;
  ccli: string | null;
  timesShown: number;
  /** Epoch ms of the last time it went live. */
  lastShownAt: number;
}

export interface LibraryExportOptions {
  includeBibles: boolean;
  includeUsage: boolean;
}

export interface LibraryImportReport {
  songsAdded: number;
  songsUpdated: number;
  songsSkipped: number;
  themes: number;
  playlists: number;
  settings: number;
  bibles: number;
  usage: number;
}

export interface HostApi {
  // Settings (key/value, persisted by the host).
  getSetting(key: string): Promise<string | null>;
  setSetting(key: string, value: string): Promise<void>;
  /** Playlist currently loaded as the show (null when none). */
  getActivePlaylistId(): Promise<PlaylistId | null>;

  // Library writes.
  saveSong(song: Song): Promise<Song>;
  deleteSong(id: SongId): Promise<void>;
  importSongs(files: SongImportFile[]): Promise<SongImportResult>;
  savePlaylist(playlist: Playlist): Promise<Playlist>;
  deletePlaylist(id: PlaylistId): Promise<void>;
  saveTheme(theme: Theme): Promise<Theme>;
  deleteTheme(id: ThemeId): Promise<void>;

  // Bible versions.
  listBibleCatalog(): Promise<BibleCatalogEntry[]>;
  installBible(code: VersionCode): Promise<BibleVersion>;
  importBible(req: BibleImportRequest): Promise<BibleVersion>;
  removeBible(code: VersionCode): Promise<void>;
  updateBibleVersion(code: VersionCode, patch: BibleVersionPatch): Promise<BibleVersion[]>;
  /** Persists the presentation order (`sortOrder`) of the installed versions. */
  reorderBibleVersions(codes: VersionCode[]): Promise<BibleVersion[]>;

  // Displays.
  listMonitors(): Promise<MonitorInfo[]>;
  /** Opens (or moves) the output window `id` on monitor `monitor` (null = automatic). */
  openOutput(id: OutputId, monitor: number | null): Promise<void>;
  closeOutput(id: OutputId): Promise<void>;
  /** Monitor index each open output window currently uses. */
  outputMonitors(): Promise<Record<OutputId, number | null>>;

  // Remote control.
  getPairing(): Promise<PairingInfo>;
  regeneratePin(): Promise<PairingInfo>;
  /** Role the next PINs grant; invalidates the current PIN. */
  setPairingRole(role: PairingRole): Promise<PairingInfo>;
  setRemoteEnabled(enabled: boolean): Promise<PairingInfo>;
  listDevices(): Promise<RemoteDevice[]>;
  kickDevice(id: string): Promise<void>;

  // CCLI report and library backup.
  /** Songs shown in `[fromMs, toMs)`, most shown first. */
  usageReport(fromMs: number, toMs: number): Promise<UsageRow[]>;
  /** Writes the whole library to a JSON file and returns its path. */
  exportLibrary(opts: LibraryExportOptions): Promise<string>;
  /** `replace` wipes songs, themes, services and settings first; otherwise merges by id. */
  importLibrary(json: string, replace: boolean): Promise<LibraryImportReport>;

  health(): Promise<HealthInfo>;
  startupInfo(): Promise<StartupInfo>;
}

/**
 * IPC command name per method for the Tauri adapter. The shell (`apps/desktop/src-tauri`) owns the
 * implementation; `list_monitors`, `open_output` and `close_window` already exist there.
 */
export const HOST_COMMANDS = {
  getSetting: "get_setting",
  setSetting: "set_setting",
  getActivePlaylistId: "get_active_playlist_id",
  saveSong: "save_song",
  deleteSong: "delete_song",
  importSongs: "import_songs",
  savePlaylist: "save_playlist",
  deletePlaylist: "delete_playlist",
  saveTheme: "save_theme",
  deleteTheme: "delete_theme",
  listBibleCatalog: "list_bible_catalog",
  installBible: "install_bible",
  importBible: "import_bible",
  removeBible: "remove_bible",
  updateBibleVersion: "update_bible_version",
  reorderBibleVersions: "reorder_bible_versions",
  listMonitors: "list_monitors",
  openOutput: "open_output",
  closeOutput: "close_window",
  outputMonitors: "output_monitors",
  getPairing: "get_pairing",
  regeneratePin: "regenerate_pin",
  setPairingRole: "set_pairing_role",
  setRemoteEnabled: "set_remote_enabled",
  listDevices: "list_devices",
  kickDevice: "kick_device",
  usageReport: "usage_report",
  exportLibrary: "export_library",
  importLibrary: "import_library",
  health: "health",
  startupInfo: "startup_info",
} as const satisfies Record<keyof HostApi, string>;
