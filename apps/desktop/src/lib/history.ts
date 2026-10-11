import { useCallback, useRef, useState } from "react";

/** Immutable undo/redo stack with coalescing of rapid edits (typing) into one step. */
export interface History<T> {
  past: T[];
  present: T;
  future: T[];
  /** Key and timestamp of the last coalescable push. */
  lastKey: string | null;
  lastAt: number;
}

export const COALESCE_MS = 800;
const LIMIT = 200;

export function createHistory<T>(initial: T): History<T> {
  return { past: [], present: initial, future: [], lastKey: null, lastAt: 0 };
}

export function pushHistory<T>(
  h: History<T>,
  next: T,
  key: string | null = null,
  now = Date.now(),
): History<T> {
  if (Object.is(next, h.present)) return h;
  const coalesce =
    key !== null && key === h.lastKey && now - h.lastAt < COALESCE_MS && h.past.length > 0;
  const past = coalesce ? h.past : [...h.past, h.present].slice(-LIMIT);
  return { past, present: next, future: [], lastKey: key, lastAt: now };
}

export function undoHistory<T>(h: History<T>): History<T> {
  if (h.past.length === 0) return h;
  const prev = h.past[h.past.length - 1]!;
  return {
    past: h.past.slice(0, -1),
    present: prev,
    future: [h.present, ...h.future],
    lastKey: null,
    lastAt: 0,
  };
}

export function redoHistory<T>(h: History<T>): History<T> {
  if (h.future.length === 0) return h;
  const [next, ...rest] = h.future as [T, ...T[]];
  return { past: [...h.past, h.present], present: next, future: rest, lastKey: null, lastAt: 0 };
}

export interface UseHistory<T> {
  value: T;
  canUndo: boolean;
  canRedo: boolean;
  /** Replaces the present as a new undo step. `key` coalesces consecutive edits of one field. */
  set: (next: T | ((cur: T) => T), key?: string) => void;
  undo: () => T | undefined;
  redo: () => T | undefined;
  /** Replaces everything (loading another document). */
  reset: (value: T) => void;
}

export function useHistory<T>(initial: T): UseHistory<T> {
  const [h, setH] = useState(() => createHistory(initial));
  const ref = useRef(h);
  ref.current = h;

  const commit = useCallback((next: History<T>) => {
    ref.current = next;
    setH(next);
  }, []);

  return {
    value: h.present,
    canUndo: h.past.length > 0,
    canRedo: h.future.length > 0,
    set: (next, key) => {
      const cur = ref.current;
      const value = typeof next === "function" ? (next as (c: T) => T)(cur.present) : next;
      commit(pushHistory(cur, value, key ?? null));
    },
    undo: () => {
      const n = undoHistory(ref.current);
      commit(n);
      return n.present;
    },
    redo: () => {
      const n = redoHistory(ref.current);
      commit(n);
      return n.present;
    },
    reset: (value) => commit(createHistory(value)),
  };
}
