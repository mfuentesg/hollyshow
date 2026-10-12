import {
  type Ack,
  type BibleVersion,
  type Command,
  type Playlist,
  type PlaylistSummary,
  type SongSummary,
  type Theme,
  isStaleCommandCode,
} from "@hollyshow/protocol";
import { defaultTheme } from "@hollyshow/slide-renderer";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import { useApi, useHost } from "../api/context";
import type { HealthInfo, PairingInfo, RemoteDevice } from "../api/host";
import { usePresentation, type Presentation } from "../hooks/usePresentation";
import { cmd } from "../lib/cmd";
import { useHistory } from "../lib/history";
import { instantiate, toTemplate } from "../lib/playlistOps";
import { parseClock } from "../lib/schedule";
import { errorText, useToasts } from "../hooks/useToasts";

export interface OperatorCtx {
  p: Presentation;
  playlist: Playlist | null;
  playlists: PlaylistSummary[];
  /** Song library index (title, author, key) for subtitles and quick pickers. */
  songs: SongSummary[];
  themes: Theme[];
  /** Installed Bible versions in presentation order. */
  versions: BibleVersion[];
  /** Theme used to draw slides in the monitors for the live cue. */
  themeFor: (themeId: string | null | undefined) => Theme;
  selectedItemId: string | null;
  selectItem: (id: string | null) => void;
  send: (cmd: Command) => Promise<Ack | null>;
  /** Applies a pure edit to the active playlist, with undo history and autosave. */
  editPlaylist: (mutate: (p: Playlist) => Playlist, coalesceKey?: string) => void;
  undo: () => void;
  redo: () => void;
  canUndo: boolean;
  canRedo: boolean;
  openPlaylist: (id: string) => Promise<void>;
  createFromTemplate: (templateId: string, name: string, date: string | null) => Promise<void>;
  saveAsTemplate: (name: string) => Promise<void>;
  /** Service start (seconds since midnight) used for the schedule. */
  startSecs: number;
  setStartTime: (hhmm: string) => void;
  savedAt: number | null;
  paletteOpen: boolean;
  setPaletteOpen: (open: boolean) => void;
  devices: RemoteDevice[];
  pairing: PairingInfo | null;
  health: HealthInfo | null;
  reloadLibrary: () => Promise<void>;
  /** Re-reads devices, pairing and health now (after kicking a device, regenerating the PIN...). */
  refreshHost: () => Promise<void>;
}

const Ctx = createContext<OperatorCtx | null>(null);

export function useOperator(): OperatorCtx {
  const v = useContext(Ctx);
  if (!v) throw new Error("useOperator must be used inside <OperatorProvider>");
  return v;
}

const START_KEY = "service.startTime";

export function OperatorProvider({ children }: { children: ReactNode }) {
  const api = useApi();
  const host = useHost();
  const toasts = useToasts();
  const { t } = useTranslation();
  const p = usePresentation(api);

  const pl = useHistory<Playlist | null>(null);
  const [playlists, setPlaylists] = useState<PlaylistSummary[]>([]);
  const [songs, setSongs] = useState<SongSummary[]>([]);
  const [themes, setThemes] = useState<Theme[]>([]);
  const [versions, setVersions] = useState<BibleVersion[]>([]);
  const [selectedItemId, setSelected] = useState<string | null>(null);
  const [startSecs, setStartSecs] = useState(10 * 3600);
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [devices, setDevices] = useState<RemoteDevice[]>([]);
  const [pairing, setPairing] = useState<PairingInfo | null>(null);
  const [health, setHealth] = useState<HealthInfo | null>(null);
  const plRef = useRef(pl);
  plRef.current = pl;
  const showRevision = p?.view.showRevision ?? -1;

  const reloadLibrary = useCallback(async () => {
    const [lists, sg, th, bv] = await Promise.all([
      api.query({ kind: "listPlaylists" }),
      api.query({ kind: "searchSongs", text: "", limit: 1000 }),
      api.query({ kind: "listThemes" }),
      api.query({ kind: "listBibleVersions" }),
    ]);
    setPlaylists(lists.items);
    setSongs(sg.items);
    setThemes(th.items);
    setVersions(bv.items.filter((v) => v.isEnabled).sort((a, b) => a.sortOrder - b.sortOrder));
  }, [api]);

  const loadActive = useCallback(async () => {
    const id = await host.getActivePlaylistId();
    if (!id) {
      plRef.current.reset(null);
      return;
    }
    const r = await api.query({ kind: "getPlaylist", id });
    const cur = plRef.current.value;
    if (r.playlist && JSON.stringify(r.playlist) !== JSON.stringify(cur)) {
      if (cur?.id === r.playlist.id) plRef.current.set(r.playlist);
      else plRef.current.reset(r.playlist);
    }
  }, [api, host]);

  // Initial load and settings.
  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        await reloadLibrary();
        const start = await host.getSetting(START_KEY);
        if (alive && start) setStartSecs(parseClock(start));
      } catch (e) {
        toasts.error(e);
      }
    })();
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api, host]);

  // The active playlist follows the show: reload whenever the host rebuilds it.
  useEffect(() => {
    if (showRevision < 0) return;
    loadActive().catch(toasts.error);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showRevision, loadActive]);

  // Library events from other clients or the host.
  useEffect(
    () =>
      api.subscribe((u) => {
        for (const ev of u.events) {
          if (ev.kind === "libraryChanged" || ev.kind === "playlistChanged") {
            reloadLibrary().catch(() => {});
            if (ev.kind === "playlistChanged") loadActive().catch(() => {});
          }
        }
      }),
    [api, reloadLibrary, loadActive],
  );

  // Remote devices and health: slow polling, tolerant to a host without the commands.
  const aliveRef = useRef(true);
  const refreshHost = useCallback(async () => {
    const [d, pi, h] = await Promise.allSettled([
      host.listDevices(),
      host.getPairing(),
      host.health(),
    ]);
    if (!aliveRef.current) return;
    if (d.status === "fulfilled") setDevices(d.value);
    if (pi.status === "fulfilled") setPairing(pi.value);
    if (h.status === "fulfilled") setHealth(h.value);
  }, [host]);
  useEffect(() => {
    aliveRef.current = true;
    void refreshHost();
    const id = setInterval(() => void refreshHost(), 5000);
    return () => {
      aliveRef.current = false;
      clearInterval(id);
    };
  }, [refreshHost]);

  const send = useCallback(
    async (cmd: Command): Promise<Ack | null> => {
      try {
        const ack = await api.dispatch(cmd);
        if (!ack.ok && ack.error && !isStaleCommandCode(ack.error.code)) {
          toasts.push(
            ack.error.code === "locked"
              ? t("errors.locked")
              : `${ack.error.code}: ${ack.error.message}`,
            "error",
          );
        }
        return ack;
      } catch (e) {
        toasts.push(errorText(e), "error");
        return null;
      }
    },
    [api, toasts, t],
  );

  const persist = useCallback(
    async (next: Playlist) => {
      try {
        await host.savePlaylist(next);
        setSavedAt(Date.now());
      } catch (e) {
        toasts.error(e);
      }
    },
    [host, toasts],
  );

  const editPlaylist = useCallback(
    (mutate: (p: Playlist) => Playlist, coalesceKey?: string) => {
      const cur = plRef.current.value;
      if (!cur) return;
      const next = mutate(cur);
      if (next === cur) return;
      plRef.current.set(next, coalesceKey);
      void persist(next);
    },
    [persist],
  );

  const undo = useCallback(() => {
    if (!plRef.current.canUndo) return;
    const prev = plRef.current.undo();
    if (prev) void persist(prev);
  }, [persist]);
  const redo = useCallback(() => {
    if (!plRef.current.canRedo) return;
    const nxt = plRef.current.redo();
    if (nxt) void persist(nxt);
  }, [persist]);

  const openPlaylist = useCallback(
    async (id: string) => {
      const ack = await send(cmd.setActivePlaylist(id));
      if (ack?.ok) {
        const r = await api.query({ kind: "getPlaylist", id });
        pl.reset(r.playlist);
        setSelected(null);
        await reloadLibrary();
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [api, send, reloadLibrary],
  );

  const createFromTemplate = useCallback(
    async (templateId: string, name: string, date: string | null) => {
      const r = await api.query({ kind: "getPlaylist", id: templateId });
      if (!r.playlist) {
        toasts.push(t("errors.notFound"), "error");
        return;
      }
      const svc = instantiate(r.playlist, name, date);
      await host.savePlaylist(svc);
      await openPlaylist(svc.id);
    },
    [api, host, openPlaylist, toasts, t],
  );

  const saveAsTemplate = useCallback(
    async (name: string) => {
      const cur = plRef.current.value;
      if (!cur) return;
      await host.savePlaylist(toTemplate(cur, name));
      await reloadLibrary();
      toasts.push(t("service.templateSaved", { name }), "success");
    },
    [host, reloadLibrary, toasts, t],
  );

  const setStartTime = useCallback(
    (hhmm: string) => {
      setStartSecs(parseClock(hhmm, startSecs));
      void host.setSetting(START_KEY, hhmm).catch(() => {});
    },
    [host, startSecs],
  );

  const themeMap = useMemo(() => new Map(themes.map((x) => [x.id, x])), [themes]);
  const themeFor = useCallback(
    (id: string | null | undefined) =>
      (id ? themeMap.get(id) : undefined) ?? themes[0] ?? defaultTheme,
    [themeMap, themes],
  );

  const value = useMemo<OperatorCtx | null>(
    () =>
      p
        ? {
            p,
            playlist: pl.value,
            playlists,
            songs,
            themes,
            versions,
            themeFor,
            selectedItemId,
            selectItem: setSelected,
            send,
            editPlaylist,
            undo,
            redo,
            canUndo: pl.canUndo,
            canRedo: pl.canRedo,
            openPlaylist,
            createFromTemplate,
            saveAsTemplate,
            startSecs,
            setStartTime,
            savedAt,
            paletteOpen,
            setPaletteOpen,
            devices,
            pairing,
            health,
            reloadLibrary,
            refreshHost,
          }
        : null,
    [
      p,
      pl.value,
      pl.canUndo,
      pl.canRedo,
      playlists,
      songs,
      themes,
      versions,
      themeFor,
      selectedItemId,
      send,
      editPlaylist,
      undo,
      redo,
      openPlaylist,
      createFromTemplate,
      saveAsTemplate,
      startSecs,
      setStartTime,
      savedAt,
      paletteOpen,
      devices,
      pairing,
      health,
      reloadLibrary,
      refreshHost,
    ],
  );

  if (!value) return null;
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
