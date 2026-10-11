import type { ApiUpdate, HollyApi } from "@hollyshow/protocol";
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RECONCILE_MS, useLivePresentation } from "./useLivePresentation";

type Snap = { version: number; showRevision: number; show: unknown[] } & Record<string, unknown>;

function fakeApi(initial: Snap) {
  let listener: ((u: ApiUpdate) => void) | null = null;
  const pending: Array<(s: Snap) => void> = [];
  let snap = initial;
  let manual = false;
  const api = {
    getSnapshot: vi.fn(() =>
      manual ? new Promise<Snap>((res) => pending.push(res)) : Promise.resolve(snap),
    ),
    query: vi.fn(async () => ({ kind: "themes", items: [] })),
    subscribe: (l: (u: ApiUpdate) => void) => {
      listener = l;
      return () => {
        listener = null;
      };
    },
  };
  return {
    api: api as unknown as HollyApi,
    raw: api,
    setSnap: (s: Snap) => (snap = s),
    hold: () => (manual = true),
    resolve: (i: number, s: Snap) => pending[i]!(s),
    emit: (version: number, showRevision = 0) =>
      listener?.({
        version,
        view: { version, showRevision } as ApiUpdate["view"],
        events: [],
      }),
  };
}

const snap = (version: number, showRevision = 0): Snap => ({ version, showRevision, show: [] });

describe("useLivePresentation ordering", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("ignores updates older than the shown version", async () => {
    const f = fakeApi(snap(5));
    const { result } = renderHook(() => useLivePresentation(f.api));
    await act(async () => {});
    act(() => void f.emit(7));
    act(() => void f.emit(6));
    expect(result.current?.view.version).toBe(7);
  });

  it("applies the live view immediately even when showRevision changed", async () => {
    const f = fakeApi(snap(1));
    const { result } = renderHook(() => useLivePresentation(f.api));
    await act(async () => {});
    f.hold(); // the snapshot never resolves
    act(() => void f.emit(2, 1));
    expect(result.current?.view.version).toBe(2);
    expect(result.current?.view.showRevision).toBe(1);
  });

  it("a slow snapshot never overwrites a newer update and syncs are serialized", async () => {
    const f = fakeApi(snap(1));
    const { result } = renderHook(() => useLivePresentation(f.api));
    await act(async () => {});
    f.hold();
    act(() => void f.emit(2, 1)); // sync in flight (index 0)
    act(() => void f.emit(3, 1)); // newer update: no second concurrent request
    expect(f.raw.getSnapshot).toHaveBeenCalledTimes(2); // initial + one
    await act(async () => {
      f.resolve(0, { ...snap(2, 1), show: [{ id: "a" }] });
      await Promise.resolve();
    });
    expect(result.current?.view.version).toBe(3);
    expect(result.current?.show).toEqual([{ id: "a" }]);
  });

  it("does not re-query themes per show revision", async () => {
    const f = fakeApi(snap(1));
    renderHook(() => useLivePresentation(f.api));
    await act(async () => {});
    const before = f.raw.query.mock.calls.length;
    act(() => void f.emit(2, 1));
    await act(async () => {});
    expect(f.raw.query.mock.calls.length).toBe(before);
  });

  it("reconciles periodically when the host moved on without an event", async () => {
    const f = fakeApi(snap(1));
    const { result } = renderHook(() => useLivePresentation(f.api));
    await act(async () => {});
    f.setSnap(snap(9));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(RECONCILE_MS + 10);
    });
    expect(result.current?.view.version).toBe(9);
  });
});
