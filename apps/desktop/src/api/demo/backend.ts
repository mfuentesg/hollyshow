import {
  emptyState,
  mockApply,
  type Ack,
  type ApiUpdate,
  type BibleVersion,
  type Command,
  type HollyApi,
  type MonitorInfo,
  type Passage,
  type PresentationState,
  type Playlist,
  type QueryRequest,
  type QueryResponse,
  type QueryResult,
  type ShowEntry,
  type Slide,
  type Song,
  type Theme,
  type Verse,
} from "@hollyshow/protocol";
import { composeBibleSlide, defaultBibleOptions } from "../../lib/bibleSlide";
import { fold } from "../../lib/text";
import { songSlides } from "../../lib/songOps";
import type {
  BibleCatalogEntry,
  BibleImportRequest,
  HealthInfo,
  HostApi,
  LibraryImportReport,
  UsageRow,
  PairingInfo,
  RemoteDevice,
  SongImportFile,
} from "../host";
import {
  sampleBibleCatalog,
  sampleBibleVersions,
  samplePlaylists,
  sampleSongs,
  sampleThemes,
  sampleVerses,
} from "./sample";

export interface DemoSeed {
  songs?: Song[];
  playlists?: Playlist[];
  themes?: Theme[];
  bibleVersions?: BibleVersion[];
  verses?: Record<string, Verse[]>;
  activePlaylistId?: string | null;
  devices?: RemoteDevice[];
  monitors?: MonitorInfo[];
  /** Clock used for timers and the item clock (tests inject a fake). */
  now?: () => number;
}

export interface DemoBackend {
  api: HollyApi;
  host: HostApi;
  readonly state: PresentationState;
}

const LOCKED = new Set(["updateSlide", "setOutputLook", "removeOutput"]);

const ord = (r: { chapter: number; verse: number | null }) => r.chapter * 1000 + (r.verse ?? 0);

function inPassage(v: Verse, p: Passage): boolean {
  if (v.reference.book !== p.start.book) return false;
  const lo = p.start.chapter * 1000 + (p.start.verse ?? 0);
  const hi = p.end.chapter * 1000 + (p.end.verse ?? 999);
  const o = ord(v.reference);
  return o >= lo && o <= hi;
}

/**
 * In-memory implementation of `HollyApi` + `HostApi` for `pnpm dev:web` and component tests. It
 * models the host closely enough to exercise the whole operator UI: playlist -> show expansion,
 * live edits, timers, output looks, Bible installation, pairing and monitors. The Rust reducer
 * stays the source of truth in the real app.
 */
export function createDemoBackend(seed: DemoSeed = {}): DemoBackend {
  const now = seed.now ?? (() => Date.now());
  const songs = new Map((seed.songs ?? sampleSongs()).map((s) => [s.id, s]));
  const playlists = new Map((seed.playlists ?? samplePlaylists()).map((p) => [p.id, p]));
  const themes = new Map((seed.themes ?? sampleThemes()).map((t) => [t.id, t]));
  let versions: BibleVersion[] = [...(seed.bibleVersions ?? sampleBibleVersions())];
  const verses = new Map(Object.entries(seed.verses ?? sampleVerses()));
  const catalog: BibleCatalogEntry[] = sampleBibleCatalog();
  const settings = new Map<string, string>();
  /** Demo usage log: every song shown twice, a week and a day ago. */
  const usageLog = [...songs.values()].flatMap((song) => [
    { songId: song.id, shownAt: Date.now() - 7 * 86_400_000 },
    { songId: song.id, shownAt: Date.now() - 86_400_000 },
  ]);
  let activeId: string | null =
    seed.activePlaylistId === undefined ? "svc-main" : seed.activePlaylistId;
  let devices: RemoteDevice[] = seed.devices ?? [
    {
      id: "dev-1",
      name: "iPhone de Ana",
      role: "operator",
      pairedAtMs: now() - 3600_000,
      lastSeenMs: now() - 2000,
      connected: true,
    },
    {
      id: "dev-2",
      name: "Pixel de Luis",
      role: "pastor",
      pairedAtMs: now() - 7200_000,
      lastSeenMs: now() - 9000,
      connected: true,
    },
    {
      id: "dev-3",
      name: "iPad escenario",
      role: "stage",
      pairedAtMs: now() - 86400_000,
      lastSeenMs: now() - 600_000,
      connected: false,
    },
  ];
  const monitors: MonitorInfo[] = seed.monitors ?? [
    {
      index: 0,
      name: "Pantalla integrada",
      x: 0,
      y: 0,
      width: 1440,
      height: 900,
      scaleFactor: 2,
      isPrimary: true,
    },
    {
      index: 1,
      name: "Proyector HDMI",
      x: 1440,
      y: 0,
      width: 1920,
      height: 1080,
      scaleFactor: 1,
      isPrimary: false,
    },
  ];
  const outputMon: Record<string, number | null> = {};
  let pairing: PairingInfo = {
    enabled: true,
    url: "http://hollyshow.local:7420",
    pin: "418266",
    pinExpiresMs: null,
  };
  let pinCounter = 0;

  let state: PresentationState = { ...emptyState() };
  const liveEdits = new Map<string, Slide>();
  const listeners = new Set<(u: ApiUpdate) => void>();
  const log: Command[] = [];

  const themeOf = (id: string | null) =>
    (id ? themes.get(id) : undefined) ?? themes.values().next().value!;

  function versesFor(code: string, p: Passage): Verse[] {
    return (verses.get(code) ?? []).filter((v) => inPassage(v, p));
  }
  function bibleSlide(id: string, p: Passage, codes: string[], layout: Slide["layout"]): Slide {
    const texts = codes.map((c) => {
      const meta = versions.find((x) => x.code === c);
      return {
        code: c,
        language: meta?.language ?? "es",
        copyright: meta?.copyrightText ?? null,
        verses: versesFor(c, p),
      };
    });
    return composeBibleSlide(id, p, texts, layout, defaultBibleOptions);
  }

  function expand(pl: Playlist): ShowEntry[] {
    return pl.items.map((it): ShowEntry => {
      const c = it.content;
      let slides: Slide[] = [];
      const mk = (
        i: number,
        label: string | null,
        lines: string[],
        extra: Partial<Slide> = {},
      ): Slide => ({
        id: `${it.id}:${i}`,
        kind: "lyrics",
        label,
        lines,
        translation: [],
        chart: [],
        bible: [],
        layout: "columns",
        media: null,
        footer: null,
        themeId: it.themeId,
        sizeGroup: null,
        ...extra,
      });
      switch (c.kind) {
        case "song": {
          const song = songs.get(c.songId);
          if (!song) break;
          slides = songSlides(song, {
            arrangementId: c.arrangementId,
            maxLines: themeOf(it.themeId ?? song.themeId).maxLinesPerSlide,
            transpose: c.transpose,
            copyright: "first",
            themeId: it.themeId,
            idPrefix: it.id,
          });
          break;
        }
        case "bible":
          slides = c.versions.length
            ? [bibleSlide(`${it.id}:0`, c.passage, c.versions, c.layout)]
            : [];
          break;
        case "text":
          slides = [mk(0, null, c.body.split("\n"), { kind: "text" })];
          break;
        case "countdown":
          slides = [
            mk(
              0,
              null,
              [`${Math.floor(c.seconds / 60)}:${String(c.seconds % 60).padStart(2, "0")}`],
              { kind: "countdown" },
            ),
          ];
          break;
        case "media":
          slides = [mk(0, null, [], { kind: "media", media: c.mediaId })];
          break;
        case "header":
          break;
      }
      slides = slides.map((s) => liveEdits.get(s.id) ?? s);
      return { itemId: it.id, title: it.title, slides, durationSecs: it.durationSecs };
    });
  }

  function emit(next: PresentationState) {
    state = next;
    const { show: _show, ...view } = state;
    const u: ApiUpdate = { version: state.version, view, events: [] };
    listeners.forEach((l) => l(u));
  }
  function bump(patch: Partial<PresentationState>): PresentationState {
    return { ...state, ...patch, version: state.version + 1 };
  }
  function reloadShow() {
    const pl = activeId ? playlists.get(activeId) : undefined;
    const show = pl ? expand(pl) : [];
    // keep the live/preview cues pointing at refreshed slides when ids still exist
    const refresh = (c: PresentationState["live"]) => {
      if (!c || c.itemId === null) return c;
      const e = show.find((x) => x.itemId === c.itemId);
      const s = e?.slides[c.slideIndex];
      return s ? { ...c, slide: s } : c;
    };
    emit(
      bump({
        show,
        showRevision: state.showRevision + 1,
        live: refresh(state.live),
        preview: refresh(state.preview),
      }),
    );
  }
  const ack = (ok: boolean, code?: string, message = ""): Ack => ({
    ok,
    version: state.version,
    duplicate: false,
    error: ok ? null : { code: code ?? "invalid", message },
  });

  function startItemClock(prev: PresentationState, next: PresentationState): PresentationState {
    const id = next.live?.itemId ?? null;
    if (id === (prev.live?.itemId ?? null)) return next;
    const entry = next.show.find((e) => e.itemId === id);
    return {
      ...next,
      itemClock: entry?.durationSecs
        ? { itemId: entry.itemId, plannedSecs: entry.durationSecs, startedAtMs: now() }
        : null,
    };
  }

  async function dispatch(cmd: Command): Promise<Ack> {
    log.push(cmd);
    if (state.serviceMode && LOCKED.has(cmd.kind)) return ack(false, "locked", "service mode");
    switch (cmd.kind) {
      case "setActivePlaylist": {
        if (!playlists.has(cmd.id)) return ack(false, "not_found", cmd.id);
        activeId = cmd.id;
        liveEdits.clear();
        reloadShow();
        return ack(true);
      }
      case "refreshShow":
        reloadShow();
        return ack(true);
      case "showBible": {
        if (cmd.versions.length === 0 || cmd.versions.length > 4)
          return ack(false, "invalid", "1..4 versions");
        const slide = bibleSlide(
          `bible:${state.version + 1}`,
          cmd.passage,
          cmd.versions,
          cmd.layout,
        );
        const cue = { itemId: null, slideIndex: 0, slide };
        emit(
          cmd.target === "live"
            ? bump({ live: cue, clear: false, logo: false, itemClock: null })
            : bump({ preview: cue }),
        );
        return ack(true);
      }
      case "updateSlide": {
        const e = state.show.find((x) => x.itemId === cmd.itemId);
        if (!e || cmd.slideIndex >= e.slides.length) return ack(false, "unknown_item", cmd.itemId);
        const old = e.slides[cmd.slideIndex]!;
        const slide = { ...cmd.slide, id: old.id };
        liveEdits.set(old.id, slide);
        const show = state.show.map((x) =>
          x.itemId === cmd.itemId
            ? { ...x, slides: x.slides.map((s, i) => (i === cmd.slideIndex ? slide : s)) }
            : x,
        );
        const swap = (c: PresentationState["live"]) =>
          c && c.itemId === cmd.itemId && c.slideIndex === cmd.slideIndex ? { ...c, slide } : c;
        emit(bump({ show, live: swap(state.live), preview: swap(state.preview) }));
        return ack(true);
      }
      case "setTimer": {
        const t = { ...cmd.timer, run: "stopped" as const, startedAtMs: null, elapsedMs: 0 };
        const rest = state.timers.filter((x) => x.id !== t.id);
        const prev = state.timers.find((x) => x.id === t.id);
        emit(
          bump({
            timers: [
              ...rest,
              prev ? { ...prev, label: t.label, kind: t.kind, durationSecs: t.durationSecs } : t,
            ],
          }),
        );
        return ack(true);
      }
      case "startTimer":
      case "pauseTimer":
      case "resetTimer":
      case "removeTimer": {
        const t = state.timers.find((x) => x.id === cmd.id);
        if (!t) return ack(false, "unknown_timer", cmd.id);
        const t0 = now();
        const elapsed =
          t.run === "running" && t.startedAtMs !== null
            ? t.elapsedMs + (t0 - t.startedAtMs)
            : t.elapsedMs;
        let timers = state.timers;
        if (cmd.kind === "startTimer")
          timers = timers.map((x) =>
            x.id === t.id ? { ...x, run: "running", startedAtMs: t0 } : x,
          );
        if (cmd.kind === "pauseTimer")
          timers = timers.map((x) =>
            x.id === t.id ? { ...x, run: "paused", startedAtMs: null, elapsedMs: elapsed } : x,
          );
        if (cmd.kind === "resetTimer")
          timers = timers.map((x) =>
            x.id === t.id ? { ...x, run: "stopped", startedAtMs: null, elapsedMs: 0 } : x,
          );
        if (cmd.kind === "removeTimer") timers = timers.filter((x) => x.id !== t.id);
        emit(bump({ timers }));
        return ack(true);
      }
      case "setOutputLook": {
        emit(
          bump({
            outputs: { ...state.outputs, [cmd.output]: cmd.look },
            outputLayers: state.outputLayers[cmd.output]
              ? state.outputLayers
              : {
                  ...state.outputLayers,
                  [cmd.output]: {
                    background: null,
                    media: null,
                    text: true,
                    overlay: null,
                    blackout: false,
                  },
                },
          }),
        );
        return ack(true);
      }
      case "setOutputLayers":
        emit(bump({ outputLayers: { ...state.outputLayers, [cmd.output]: cmd.layers } }));
        return ack(true);
      case "removeOutput": {
        if (cmd.output === "main") return ack(false, "invalid", "main cannot be removed");
        const { [cmd.output]: _o, ...outputs } = state.outputs;
        const { [cmd.output]: _l, ...outputLayers } = state.outputLayers;
        emit(bump({ outputs, outputLayers }));
        return ack(true);
      }
      case "resetItemClock":
        emit(
          bump({ itemClock: state.itemClock ? { ...state.itemClock, startedAtMs: now() } : null }),
        );
        return ack(true);
      default: {
        const r = mockApply(state, cmd);
        if (!r.ok) return ack(false, r.code, r.message);
        if (r.state !== state) emit(startItemClock(state, r.state));
        return ack(true);
      }
    }
  }

  const norm = fold;
  async function query<Q extends QueryRequest>(q: Q): Promise<QueryResult<Q>> {
    let res: QueryResponse;
    switch (q.kind) {
      case "searchSongs": {
        const needle = norm(q.text).trim();
        const all = [...songs.values()];
        const hits = all.filter((s) => {
          if (!needle) return true;
          const hay = norm(
            [
              s.title,
              s.altTitle ?? "",
              s.author ?? "",
              s.tags.join(" "),
              s.sections.map((x) => x.lyrics).join(" "),
            ].join(" "),
          );
          return needle.split(/\s+/).every((t) => hay.includes(t));
        });
        res = {
          kind: "songs",
          items: hits.slice(0, q.limit).map((s) => {
            const sec = needle
              ? s.sections.find((x) => norm(x.lyrics).includes(needle.split(/\s+/)[0]!))
              : undefined;
            const inTitle = norm(s.title).includes(needle);
            return {
              id: s.id,
              title: s.title,
              author: s.author,
              key: s.key,
              snippet: !inTitle && sec ? (sec.lyrics.split("\n")[0] ?? null) : null,
            };
          }),
        };
        break;
      }
      case "getSong":
        res = { kind: "song", song: songs.get(q.id) ?? null };
        break;
      case "listPlaylists":
        res = {
          kind: "playlists",
          items: [...playlists.values()].map((p) => ({
            id: p.id,
            name: p.name,
            date: p.date,
            itemCount: p.items.length,
            isTemplate: p.isTemplate,
          })),
        };
        break;
      case "getPlaylist":
        res = { kind: "playlist", playlist: playlists.get(q.id) ?? null };
        break;
      case "listBibleVersions":
        res = {
          kind: "bibleVersions",
          items: [...versions].sort((a, b) => a.sortOrder - b.sortOrder),
        };
        break;
      case "getPassage":
        res = { kind: "passage", verses: versesFor(q.version, q.passage) };
        break;
      case "searchBible": {
        const needle = norm(q.text).trim();
        const items = [...verses.entries()]
          .filter(([code]) => !q.version || q.version === code)
          .flatMap(([code, vs]) =>
            vs
              .filter((v) => needle && norm(v.text).includes(needle))
              .map((v) => ({ version: code, reference: v.reference, text: v.text })),
          );
        res = { kind: "bibleHits", items: items.slice(0, q.limit) };
        break;
      }
      case "listThemes":
        res = { kind: "themes", items: [...themes.values()] };
        break;
      case "unknown":
        res = { kind: "unknown" };
        break;
    }
    return res as QueryResult<Q>;
  }

  const api: HollyApi = {
    transport: "mock",
    dispatch,
    query,
    async getSnapshot() {
      return state;
    },
    subscribe(l) {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    onConnection(l) {
      l("open");
      return () => {};
    },
    close() {
      listeners.clear();
    },
  };

  const touch = () => {
    // Library writes refresh the show when the active playlist may be affected.
    reloadShow();
  };

  const host: HostApi = {
    async getSetting(k) {
      return settings.get(k) ?? null;
    },
    async setSetting(k, v) {
      settings.set(k, v);
    },
    async getActivePlaylistId() {
      return activeId;
    },
    async saveSong(song) {
      const saved = { ...song, updatedAt: now() };
      songs.set(saved.id, saved);
      touch();
      return saved;
    },
    async deleteSong(id) {
      songs.delete(id);
      touch();
    },
    async importSongs(files: SongImportFile[]) {
      const imported: Song[] = [];
      const failed: [string, string][] = [];
      for (const f of files) {
        const text = f.content.trim();
        if (!text) {
          failed.push([f.name, "empty"]);
          continue;
        }
        const title = f.name.replace(/\.[^.]+$/, "") || "Importada";
        const t = now();
        const id = `song-imp-${t}-${imported.length}`;
        imported.push({
          id,
          title,
          altTitle: null,
          author: null,
          copyright: null,
          ccli: null,
          key: null,
          bpm: null,
          tags: [],
          themeId: null,
          sections: [
            { id: "v1", kind: "verse", label: "Verso 1", lyrics: text, translation: null },
          ],
          arrangements: [{ id: "a1", name: "Original", sequence: ["v1"] }],
          createdAt: t,
          updatedAt: t,
        });
      }
      imported.forEach((s) => songs.set(s.id, s));
      return { imported, failed };
    },
    async savePlaylist(p) {
      playlists.set(p.id, p);
      if (p.id === activeId) touch();
      return p;
    },
    async deletePlaylist(id) {
      playlists.delete(id);
      if (activeId === id) {
        activeId = null;
        touch();
      }
    },
    async saveTheme(t) {
      themes.set(t.id, t);
      touch();
      return t;
    },
    async deleteTheme(id) {
      themes.delete(id);
    },
    async listBibleCatalog() {
      return catalog.map((c) => ({ ...c, installed: versions.some((v) => v.code === c.code) }));
    },
    async installBible(code) {
      const c = catalog.find((x) => x.code === code);
      if (!c) throw new Error(`unknown bible ${code}`);
      const existing = versions.find((v) => v.code === code);
      if (existing) return existing;
      const v: BibleVersion = {
        code,
        name: c.name,
        language: c.language,
        license: c.license,
        copyrightText: c.name,
        isPublicDomain: c.isPublicDomain,
        versification: "standard",
        sortOrder: versions.length,
        isEnabled: true,
        isDefault: false,
        isSecondary: false,
      };
      versions = [...versions, v];
      return v;
    },
    async importBible(req: BibleImportRequest) {
      const code =
        (req.code ?? req.fileName.replace(/\.[^.]+$/, ""))
          .toUpperCase()
          .replace(/[^A-Z0-9]/g, "")
          .slice(0, 12) || "IMPORT";
      if (versions.some((v) => v.code === code)) throw new Error(`duplicate:${code}`);
      const v: BibleVersion = {
        code,
        name: req.name ?? code,
        language: req.language ?? "es",
        license: req.license ?? "",
        copyrightText: req.name ?? code,
        isPublicDomain: req.isPublicDomain ?? false,
        versification: "standard",
        sortOrder: versions.length,
        isEnabled: true,
        isDefault: false,
        isSecondary: false,
      };
      versions = [...versions, v];
      return v;
    },
    async removeBible(code) {
      versions = versions.filter((v) => v.code !== code);
      verses.delete(code);
    },
    async updateBibleVersion(code, patch) {
      versions = versions.map((v) => {
        if (v.code === code) return { ...v, ...patch };
        return {
          ...v,
          isDefault: patch.isDefault ? false : v.isDefault,
          isSecondary: patch.isSecondary ? false : v.isSecondary,
        };
      });
      return [...versions].sort((a, b) => a.sortOrder - b.sortOrder);
    },
    async reorderBibleVersions(codes) {
      versions = versions.map((v) => ({
        ...v,
        sortOrder: codes.indexOf(v.code) < 0 ? 999 : codes.indexOf(v.code),
      }));
      return [...versions].sort((a, b) => a.sortOrder - b.sortOrder);
    },
    async listMonitors() {
      return monitors;
    },
    async openOutput(id, monitor) {
      outputMon[id] = monitor;
    },
    async closeOutput(id) {
      delete outputMon[id];
    },
    async outputMonitors() {
      return { ...outputMon };
    },
    async getPairing() {
      return pairing;
    },
    async regeneratePin() {
      pinCounter += 1;
      pairing = { ...pairing, pin: String(100000 + ((418266 + pinCounter * 7919) % 900000)) };
      return pairing;
    },
    async setPairingRole(role) {
      settings.set("remote.pairingRole", role);
      pinCounter += 1;
      pairing = { ...pairing, pin: String(100000 + ((418266 + pinCounter * 7919) % 900000)) };
      return pairing;
    },
    async setRemoteEnabled(enabled) {
      pairing = { ...pairing, enabled };
      return pairing;
    },
    async listDevices() {
      return devices;
    },
    async kickDevice(id) {
      devices = devices.filter((d) => d.id !== id);
    },
    async usageReport(fromMs, toMs): Promise<UsageRow[]> {
      const rows = new Map<string, UsageRow>();
      for (const u of usageLog) {
        if (u.shownAt < fromMs || u.shownAt >= toMs) continue;
        const song = songs.get(u.songId);
        const row = rows.get(u.songId) ?? {
          songId: u.songId,
          title: song?.title ?? null,
          ccli: song?.ccli ?? null,
          timesShown: 0,
          lastShownAt: 0,
        };
        row.timesShown += 1;
        row.lastShownAt = Math.max(row.lastShownAt, u.shownAt);
        rows.set(u.songId, row);
      }
      return [...rows.values()].sort(
        (a, b) => b.timesShown - a.timesShown || b.lastShownAt - a.lastShownAt,
      );
    },
    async exportLibrary() {
      return "demo/exports/hollyshow-library.json";
    },
    async importLibrary(json, replace): Promise<LibraryImportReport> {
      const lib = JSON.parse(json) as { format?: string; songs?: Song[] };
      if (lib.format !== "hollyshow-library") throw { code: "invalid", message: "not a library" };
      if (replace) songs.clear();
      const list = lib.songs ?? [];
      list.forEach((s) => songs.set(s.id, s));
      touch();
      return {
        songsAdded: list.length,
        songsUpdated: 0,
        songsSkipped: 0,
        themes: 0,
        playlists: 0,
        settings: 0,
        bibles: 0,
        usage: 0,
      };
    },
    async health(): Promise<HealthInfo> {
      return { fps: 60, droppedFrames: 0, cpuPercent: 14, ramMb: 310 };
    },
    async startupInfo() {
      return { resumed: false, crashed: false, playlist: null, hadLive: false, dbRecovered: false };
    },
  };

  reloadShow();

  return {
    api,
    host,
    get state() {
      return state;
    },
  };
}
