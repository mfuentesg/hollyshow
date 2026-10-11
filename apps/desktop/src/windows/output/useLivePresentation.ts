import type { HollyApi, ShowEntry, StateView, Theme } from "@hollyshow/protocol";
import { useEffect, useState } from "react";

export interface LivePresentation {
  view: StateView;
  show: ShowEntry[];
  themes: Theme[];
}

const RETRY_MS = [250, 500, 1000, 2000, 4000];

/**
 * State source for the passive windows (output, stage). Differences from the operator hook:
 * - keeps the LAST received state forever (a dead connection or a failed query never blanks the
 *   projector: PLAN §3.1.5);
 * - retries the first snapshot with backoff instead of leaving an unhandled rejection;
 * - loads the themes and reloads them when the library says themes/media changed.
 */
export function useLivePresentation(api: HollyApi): LivePresentation | null {
  const [state, setState] = useState<LivePresentation | null>(null);

  useEffect(() => {
    let alive = true;
    let revision = -1;
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
        const { show, ...view } = snap;
        revision = view.showRevision;
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

    const off = api.subscribe((u) => {
      if (u.view.showRevision !== revision) {
        void sync();
      } else {
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
      off();
    };
  }, [api]);

  return state;
}
