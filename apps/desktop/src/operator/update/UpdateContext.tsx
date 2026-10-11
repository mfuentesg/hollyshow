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
import { errorText, useToasts } from "../../hooks/useToasts";
import { UPDATE_NS } from "./i18n";
import { applyEvent } from "./state";
import {
  DEFAULT_UPDATE_SETTINGS,
  ERR_BUSY,
  type UpdateSettingsValue,
  type UpdateState,
} from "./types";
import type { UpdateApi } from "./updateApi";

export interface UpdateCtx {
  state: UpdateState;
  currentVersion: string;
  /** Service mode or live output: restarting is not allowed, the update waits for exit. */
  busy: boolean;
  /** "Más tarde" was pressed for the ready version: the banner stays hidden. */
  bannerDismissed: boolean;
  notesOpen: boolean;
  settings: UpdateSettingsValue;
  /** Manual check; reports the result with a toast. */
  checkNow: () => Promise<void>;
  downloadNow: () => Promise<void>;
  /** Restart and update; falls back to "install on exit" when busy. */
  installNow: () => Promise<void>;
  dismissBanner: () => void;
  openNotes: () => void;
  closeNotes: () => void;
  saveSettings: (s: UpdateSettingsValue) => Promise<void>;
}

const Ctx = createContext<UpdateCtx | null>(null);

export function useUpdate(): UpdateCtx {
  const v = useContext(Ctx);
  if (!v) throw new Error("useUpdate must be used inside <UpdateProvider>");
  return v;
}

export interface UpdateProviderProps {
  api: UpdateApi;
  /** Service mode is on or an output is live (from the presentation state). */
  busy?: boolean;
  children: ReactNode;
}

/** Holds the update state machine for the operator window. Render inside `ToastProvider`. */
export function UpdateProvider({ api, busy = false, children }: UpdateProviderProps) {
  const { t } = useTranslation(UPDATE_NS);
  const toasts = useToasts();
  const [state, setState] = useState<UpdateState>({ state: "idle" });
  const [currentVersion, setCurrentVersion] = useState("");
  const [backendBusy, setBackendBusy] = useState(false);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [notesOpen, setNotesOpen] = useState(false);
  const [settings, setSettings] = useState(DEFAULT_UPDATE_SETTINGS);
  const manual = useRef(false);
  const tRef = useRef(t);
  tRef.current = t;

  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    void api
      .status()
      .then(({ currentVersion: v, busy: b, ...rest }) => {
        if (!alive) return;
        setCurrentVersion(v);
        setBackendBusy(b);
        setState((prev) => (prev.state === "idle" ? (rest as UpdateState) : prev));
      })
      .catch(() => {});
    void api
      .getSettings()
      .then((s) => alive && setSettings(s))
      .catch(() => {});
    void api
      .subscribe((ev) => {
        if (!alive) return;
        setState((prev) => applyEvent(prev, ev));
        if (ev.type === "ready") setBackendBusy(ev.deferred);
        if (ev.type === "none" && manual.current) {
          manual.current = false;
        }
      })
      .then((unsubscribe) => {
        if (alive) off = unsubscribe;
        else unsubscribe();
      });
    return () => {
      alive = false;
      off?.();
    };
  }, [api]);

  const checkNow = useCallback(async () => {
    manual.current = true;
    try {
      const r = await api.check();
      setCurrentVersion(r.currentVersion);
      if (r.state === "idle") {
        toasts.push(tRef.current("upToDate", { version: r.currentVersion }), "success");
      }
    } catch (e) {
      toasts.push(tRef.current("checkFailed", { message: errorText(e) }), "error");
    } finally {
      manual.current = false;
    }
  }, [api, toasts]);

  const downloadNow = useCallback(async () => {
    try {
      await api.download();
    } catch (e) {
      toasts.push(tRef.current("checkFailed", { message: errorText(e) }), "error");
    }
  }, [api, toasts]);

  const installNow = useCallback(async () => {
    try {
      await api.installNow();
    } catch (e) {
      if (e === ERR_BUSY || errorText(e) === ERR_BUSY) {
        setBackendBusy(true);
        toasts.push(tRef.current("installOnExitHint"), "info");
      } else {
        toasts.push(tRef.current("installFailed", { message: errorText(e) }), "error");
      }
    }
  }, [api, toasts]);

  const saveSettings = useCallback(
    async (s: UpdateSettingsValue) => {
      const before = settings;
      setSettings(s);
      try {
        setSettings(await api.setSettings(s));
        if (s.channel !== before.channel) setState({ state: "idle" });
      } catch {
        setSettings(before);
        toasts.push(tRef.current("settings.saveFailed"), "error");
      }
    },
    [api, settings, toasts],
  );

  const readyVersion = state.state === "ready" ? state.version : null;
  const value = useMemo<UpdateCtx>(
    () => ({
      state,
      currentVersion,
      busy: busy || (backendBusy && state.state === "ready"),
      bannerDismissed: readyVersion !== null && dismissed === readyVersion,
      notesOpen,
      settings,
      checkNow,
      downloadNow,
      installNow,
      dismissBanner: () => setDismissed(readyVersion),
      openNotes: () => setNotesOpen(true),
      closeNotes: () => setNotesOpen(false),
      saveSettings,
    }),
    [
      state,
      currentVersion,
      busy,
      backendBusy,
      readyVersion,
      dismissed,
      notesOpen,
      settings,
      checkNow,
      downloadNow,
      installNow,
      saveSettings,
    ],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
