import type { HollyApi, ShowEntry, StateView, Theme } from "@hollyshow/protocol";
import { useEffect, useRef, useState } from "react";
import { useHostEvents } from "../../hooks/useHostEvents";

export interface LivePresentation {
  view: StateView;
  show: ShowEntry[];
  themes: Theme[];
}

const RETRY_MS = [250, 500, 1000, 2000, 4000];
/** Low-rate safety net: compares the host's version with ours in case an event was lost. */
export const RECONCILE_MS = 10_000;

/**
 * State source for the passive windows (output, stage). Differences from the operator hook:
 * - keeps the LAST received state forever (a dead connection or a failed query never blanks the
 *   projector: PLAN §3.1.5);
 * - retries the first snapshot with backoff instead of leaving an unhandled rejection;
 * - applies state monotonically: snapshots and updates older than what is already shown are
 *   dropped (IPC results and events carry no ordering guarantee), and a slow snapshot never
 *   overwrites a newer update that landed while it was in flight;
 * - reconciles every RECONCILE_MS and on focus / visibility (a parked window being shown again);
 * - resyncs immediately on the shell's `output-resync` event (the window was un-parked);
 * - loads the themes and reloads them when the library says themes/media changed.
 */
export function useLivePresentation(api: HollyApi): LivePresentation | null {
  const [state, setState] = useState<LivePresentation | null>(null);
  const resyncRef = useRef<(() => void) | null>(null);
  useHostEvents({ outputResync: () => resyncRef.current?.() });

  useEffect(() => {
    let alive = true;
    let revision = -1; // showRevision of the show currently held in state
    let applied = -1; // highest StateView.version shown so far (within `epoch`)
    let epoch = -1; // boot epoch of the host run `applied`/`revision` refer to
    let themes: Theme[] = []; // cached in the window; reloaded only on libraryChanged
    let inFlight = false;
    let again = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const loadThemes = async () => {
      try {
        const r = await api.query({ kind: "listThemes" });
        if (!alive) return;
        themes = r.items;
        setState((s) => (s ? { ...s, themes } : s));
      } catch {
        // keep the cached themes
      }
    };

    // Fetches the show in the background. Never blocks the live view: updates are applied as
    // they arrive; this only refreshes the (heavier) show list. One request at a time; if more
    // revisions arrive meanwhile, one more round runs afterwards, so results can't land out of
    // order and a flood of updates can't starve the sync.
    const sync = async (attempt = 0): Promise<void> => {
      if (inFlight) {
        again = true;
        return;
      }
      inFlight = true;
      again = false;
      let failed = false;
      try {
        const snap = await api.getSnapshot();
        if (!alive) return;
        const { show, ...view } = snap;
        if (view.epoch !== epoch) {
          // The host restarted: its counters started over, so nothing held is comparable.
          epoch = view.epoch;
          applied = -1;
          revision = -1;
        }
        const newer = view.version >= applied;
        if (newer) applied = view.version;
        const showFresh = view.showRevision >= revision;
        if (showFresh) revision = view.showRevision;
        setState((cur) => ({
          view: newer || !cur ? view : cur.view,
          show: showFresh || !cur ? show : cur.show,
          themes: cur?.themes ?? themes,
        }));
      } catch {
        failed = true;
      } finally {
        inFlight = false;
      }
      if (!alive) return;
      if (failed) {
        timer = setTimeout(
          () => void sync(attempt + 1),
          RETRY_MS[Math.min(attempt, RETRY_MS.length - 1)],
        );
      } else if (again) {
        void sync();
      }
    };
    void sync();
    void loadThemes();

    const reconcile = async () => {
      try {
        const snap = await api.getSnapshot();
        if (!alive) return;
        if (snap.epoch !== epoch || snap.version > applied || snap.showRevision !== revision) {
          void sync();
        }
      } catch {
        // keep showing the last state; the next tick retries
      }
    };
    const interval = setInterval(() => void reconcile(), RECONCILE_MS);
    const onVisible = () => {
      if (document.visibilityState !== "hidden") void reconcile();
    };
    // The Rust shell un-parked this window (its monitor is back): resync at once, whether or not
    // the webview gets a visibility/focus event.
    resyncRef.current = () => {
      void sync();
      void loadThemes();
    };
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", onVisible);

    const off = api.subscribe((u) => {
      if (u.view.epoch !== epoch) {
        void sync(); // another host run: the snapshot decides
        return;
      }
      if (u.view.version < applied) return; // stale, out-of-order delivery
      applied = u.view.version;
      // The live cue lives in the view: show it right away, whatever the show revision.
      setState((cur) => (cur ? { ...cur, view: u.view } : cur));
      if (u.view.showRevision !== revision) void sync();
      if (
        u.events.some(
          (e) => e.kind === "libraryChanged" && (e.scope === "themes" || e.scope === "media"),
        )
      ) {
        void loadThemes();
      }
    });
    return () => {
      alive = false;
      clearTimeout(timer);
      clearInterval(interval);
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", onVisible);
      resyncRef.current = null;
      off();
    };
  }, [api]);

  return state;
}
