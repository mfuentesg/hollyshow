import { useEffect, useRef } from "react";
import { inTauri } from "../api/context";

/** Events emitted by the Rust shell besides state updates. Keep in sync with src-tauri. */
export const HOST_EVENTS = {
  monitors: "hollyshow://monitors",
  closeBlocked: "hollyshow://close-blocked",
  biblesInstalled: "hollyshow://bibles-installed",
} as const;

export type HostEventName = keyof typeof HOST_EVENTS;

/**
 * Subscribes to host events inside the Tauri shell (a no-op in a plain browser and in tests).
 * Handlers may change between renders without resubscribing.
 */
export function useHostEvents(
  handlers: Partial<Record<HostEventName, (payload: unknown) => void>>,
) {
  const ref = useRef(handlers);
  ref.current = handlers;
  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    const offs: Array<() => void> = [];
    void import("@tauri-apps/api/event").then(async ({ listen }) => {
      for (const name of Object.keys(HOST_EVENTS) as HostEventName[]) {
        const off = await listen(HOST_EVENTS[name], (e) => ref.current[name]?.(e.payload));
        if (alive) offs.push(off);
        else off();
      }
    });
    return () => {
      alive = false;
      offs.forEach((off) => off());
    };
  }, []);
}
