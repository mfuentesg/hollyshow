/**
 * Open image bank (PLAN §E2). Hand written mirror of `crates/media/src/types.rs` (serde camelCase);
 * the crate has a test that pins these field names. Not generated: `generated/` stays owned by ts-rs.
 */

export type MediaSource = "openverse" | "commons";
export type MediaSourceFilter = "all" | MediaSource;

export interface MediaResult {
  source: MediaSource;
  id: string;
  title: string;
  thumbUrl: string;
  fullUrl: string;
  /** Pixels; 0 when the provider did not report them. */
  width: number;
  height: number;
  creator: string | null;
  creatorUrl: string | null;
  /** Display form: `CC BY-SA 4.0`, `CC0 1.0`, `PDM`. */
  license: string;
  licenseUrl: string | null;
  attribution: string;
  sourcePage: string;
}

export interface MediaSearchRequest {
  /** Free text, usually Spanish; the host translates it to English keywords. */
  query: string;
  source?: MediaSourceFilter;
  /** 1-based. */
  page?: number;
  /** Also allow NC/ND licenses (explicit user choice). */
  allowNoncommercial?: boolean;
}

export interface MediaSourceFailure {
  source: MediaSource;
  message: string;
  retryAfterSecs: number | null;
}

export interface MediaSearchResponse {
  results: MediaResult[];
  page: number;
  hasMore: boolean;
  translatedQuery: string;
  failures: MediaSourceFailure[];
}

export interface MediaCategory {
  id: string;
  labelEs: string;
  labelEn: string;
  /** Spanish query put in the search box. */
  queryEs: string;
  queryEn: string;
}

/** An image stored in the local media library. `id` is the `mediaId` a theme background references. */
export interface MediaLibraryItem {
  id: string;
  path: string;
  thumbPath: string;
  width: number;
  height: number;
  bytes: number;
  downloadedAtMs: number;
  result: MediaResult;
}

/** The optional image bank methods of `HollyApi` (adapters without a host leave them undefined). */
export interface MediaApi {
  mediaSearch(req: MediaSearchRequest): Promise<MediaSearchResponse>;
  mediaCategories(): Promise<MediaCategory[]>;
  mediaDownload(result: MediaResult, allowNoncommercial?: boolean): Promise<MediaLibraryItem>;
  mediaList(): Promise<MediaLibraryItem[]>;
  mediaRemove(id: string): Promise<boolean>;
  /** Markdown "Créditos de imágenes" page (all downloaded images when `ids` is omitted). */
  mediaCredits(ids?: string[]): Promise<string>;
}

/** IPC command names of the Tauri adapter (implemented by `hollyshow_media::MediaService`). */
export const MEDIA_COMMANDS = {
  mediaSearch: "media_search",
  mediaCategories: "media_categories",
  mediaDownload: "media_download",
  mediaList: "media_list",
  mediaRemove: "media_remove",
  mediaCredits: "media_credits",
} as const satisfies Record<keyof MediaApi, string>;
