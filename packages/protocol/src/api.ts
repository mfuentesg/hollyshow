import type {
  Ack,
  Command,
  Event,
  PresentationState,
  QueryRequest,
  QueryResponse,
  StateView,
} from "./generated";

export type Unsubscribe = () => void;

/** Maps each query `kind` to the `kind` of its response. */
export interface QueryResponseKinds {
  searchSongs: "songs";
  getSong: "song";
  listPlaylists: "playlists";
  getPlaylist: "playlist";
  listBibleVersions: "bibleVersions";
  getPassage: "passage";
  searchBible: "bibleHits";
  listThemes: "themes";
  unknown: "unknown";
}

export type QueryResult<Q extends QueryRequest> = Extract<
  QueryResponse,
  { kind: QueryResponseKinds[Q["kind"]] }
>;

/** Sent to subscribers after every accepted change (and host events such as library edits). */
export interface ApiUpdate {
  version: number;
  view: StateView;
  events: Event[];
}

export type ConnectionStatus = "connecting" | "open" | "reconnecting" | "closed";

/**
 * Transport-agnostic access to HollyShow. UI code depends only on this interface.
 *
 * Adapters:
 *  - `tauri`     (apps/desktop)  invoke("dispatch" | "query" | "get_snapshot") + listen("hollyshow://update")
 *  - `websocket` (apps/remote)   ClientMsg/ServerMsg over WS with reconnection and resync
 *  - `mock`      (this package)  in-memory, for UI development and tests
 */
export interface HollyApi {
  readonly transport: "tauri" | "websocket" | "mock";

  /** Sends a command. Resolves with the server verdict; rejects only on transport failure. */
  dispatch(cmd: Command): Promise<Ack>;

  /** Typed read access to the library. */
  query<Q extends QueryRequest>(q: Q): Promise<QueryResult<Q>>;

  /** Full authoritative state including the expanded show. */
  getSnapshot(): Promise<PresentationState>;

  /** Subscribes to state changes. The listener is not called with the current state. */
  subscribe(listener: (u: ApiUpdate) => void): Unsubscribe;

  /** Connection status; adapters without a connection (tauri, mock) report "open". */
  onConnection(listener: (s: ConnectionStatus) => void): Unsubscribe;

  close(): void;
}
