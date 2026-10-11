import type { HollyApi, ApiUpdate, ConnectionStatus, QueryResult } from "./api";
import { emptyState, sampleShow } from "./fixtures";
import type {
  Ack,
  BibleVersion,
  Command,
  PresentationState,
  QueryRequest,
  QueryResponse,
  Song,
  Theme,
} from "./generated";
import { mockApply } from "./reducer";

export interface MockSeed {
  state?: PresentationState;
  songs?: Song[];
  bibleVersions?: BibleVersion[];
  themes?: Theme[];
}

/** In-memory `HollyApi` for UI development (browser without Tauri) and unit tests. */
export function createMockApi(seed: MockSeed = {}): HollyApi & {
  /** Test hook: inspect the authoritative state. */
  readonly state: PresentationState;
} {
  let state: PresentationState = seed.state ?? {
    ...emptyState(),
    show: sampleShow(),
    showRevision: 1,
  };
  const listeners = new Set<(u: ApiUpdate) => void>();
  const songs = seed.songs ?? [];
  const norm = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();

  return {
    transport: "mock",
    get state() {
      return state;
    },
    async dispatch(cmd: Command): Promise<Ack> {
      const r = mockApply(state, cmd);
      if (!r.ok) {
        return {
          ok: false,
          version: state.version,
          duplicate: false,
          error: { code: r.code, message: r.message },
        };
      }
      const changed = r.state !== state;
      state = r.state;
      if (changed) {
        const { show: _s, ...view } = state;
        const update: ApiUpdate = { version: state.version, view, events: [] };
        listeners.forEach((l) => l(update));
      }
      return { ok: true, version: state.version, error: null, duplicate: false };
    },
    async query<Q extends QueryRequest>(q: Q): Promise<QueryResult<Q>> {
      let res: QueryResponse;
      switch (q.kind) {
        case "searchSongs": {
          const needle = norm(q.text);
          res = {
            kind: "songs",
            items: songs
              .filter((s) => norm(s.title).includes(needle))
              .slice(0, q.limit)
              .map((s) => ({
                id: s.id,
                title: s.title,
                author: s.author,
                key: s.key,
                snippet: null,
              })),
          };
          break;
        }
        case "getSong":
          res = { kind: "song", song: songs.find((s) => s.id === q.id) ?? null };
          break;
        case "listPlaylists":
          res = { kind: "playlists", items: [] };
          break;
        case "getPlaylist":
          res = { kind: "playlist", playlist: null };
          break;
        case "listBibleVersions":
          res = { kind: "bibleVersions", items: seed.bibleVersions ?? [] };
          break;
        case "getPassage":
          res = { kind: "passage", verses: [] };
          break;
        case "searchBible":
          res = { kind: "bibleHits", items: [] };
          break;
        case "listThemes":
          res = { kind: "themes", items: seed.themes ?? [] };
          break;
      }
      return res as QueryResult<Q>;
    },
    async getSnapshot() {
      return state;
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    onConnection(listener: (s: ConnectionStatus) => void) {
      listener("open");
      return () => {};
    },
    close() {
      listeners.clear();
    },
  };
}
