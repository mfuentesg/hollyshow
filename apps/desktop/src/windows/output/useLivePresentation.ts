import type { HollyApi, ShowEntry, StateView, Theme } from "@hollyshow/protocol";
import { useEffect, useState } from "react";

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
 * - loads the themes and reloads them when the library says themes/media changed.
 */
export function useLivePresentation(api: HollyApi): LivePresentation | null {
  const [state, setState] = useState<LivePresentation | null>(null);

  useEffect(() => {
    let alive = true;
    let revision = -1;
    let applied = -1; // highest StateView.version shown so far
    let timer: ReturnType<typeof setTimeout> | undefined;

    const loadThemes = async () => {
      try {
        const r = await api.query({ kind: "listThemes" });
        if (alive) setState((s) => (s ? { ...s, themes: r.items } : s));
        return r.items;
      } catch {
        return null;
      }
    };

    const sync = async (attempt = 0): Promise<void> => {
      try {
        const [snap, themes] = await Promise.all([
          api.getSnapshot(),
          api.query({ kind: "listThemes" }),
        ]);
        if (!alive) return;
        // A newer update landed while this snapshot was in flight: keep it, the update that
        // changed showRevision (if any) already scheduled its own sync.
        if (snap.version < applied) return;
        const { show, ...view } = snap;
        revision = view.showRevision;
        applied = snap.version;
        setState({ view, show, themes: themes.items });
      } catch {
        if (!alive) return;
        timer = setTimeout(
          () => void sync(attempt + 1),
          RETRY_MS[Math.min(attempt, RETRY_MS.length - 1)],
        );
      }
    };
    void sync();

    const reconcile = async () => {
      try {
        const snap = await api.getSnapshot();
        if (!alive) return;
        if (snap.version > applied || snap.showRevision !== revision) void sync();
      } catch {
        // keep showing the last state; the next tick retries
      }
    };
    const interval = setInterval(() => void reconcile(), RECONCILE_MS);
    const onVisible = () => {
      if (document.visibilityState !== "hidden") void reconcile();
    };
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", onVisible);

    const off = api.subscribe((u) => {
      if (u.view.version < applied) return; // stale, out-of-order delivery
      if (u.view.showRevision !== revision) {
        void sync();
      } else {
        applied = u.view.version;
        setState((cur) => (cur ? { ...cur, view: u.view } : cur));
      }
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
      off();
    };
  }, [api]);

  return state;
}
