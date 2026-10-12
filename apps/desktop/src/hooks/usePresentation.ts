import type { HollyApi, ShowEntry, StateView } from "@hollyshow/protocol";
import { useEffect, useState } from "react";

export interface Presentation {
  view: StateView;
  show: ShowEntry[];
}

/**
 * Snapshot + live updates. The cheap `StateView` arrives on every change; the (large) show is
 * re-fetched only when `showRevision` moves.
 */
export function usePresentation(api: HollyApi): Presentation | null {
  const [p, setP] = useState<Presentation | null>(null);

  useEffect(() => {
    let alive = true;
    let revision = -1;
    let epoch = -1; // boot epoch of the host the held state belongs to

    const sync = async () => {
      const snap = await api.getSnapshot();
      if (!alive) return;
      const { show, ...view } = snap;
      revision = view.showRevision;
      epoch = view.epoch;
      setP({ view, show });
    };
    void sync();

    const off = api.subscribe((u) => {
      // A new epoch means the host restarted: whatever we hold is from another run.
      if (u.view.showRevision !== revision || u.view.epoch !== epoch) {
        void sync();
      } else {
        setP((cur) => (cur ? { ...cur, view: u.view } : cur));
      }
    });
    return () => {
      alive = false;
      off();
    };
  }, [api]);

  return p;
}
