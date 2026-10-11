import type { Command, Cue, PresentationState } from "./generated";

/**
 * Minimal TypeScript mirror of `hollyshow_core::apply`, used ONLY by the mock adapter and UI
 * tests. The Rust reducer is the single source of truth; this covers navigation and toggles.
 * Commands it does not model return `unsupported`.
 */
export type MockResult =
  { ok: true; state: PresentationState } | { ok: false; code: string; message: string };

type Pos = [number, number];

function cueAt(s: PresentationState, [ei, si]: Pos): Cue {
  const e = s.show[ei]!;
  return { itemId: e.itemId, slideIndex: si, slide: e.slides[si]! };
}

function livePos(s: PresentationState): Pos | null {
  const live = s.live;
  if (!live || live.itemId === null) return null;
  const ei = s.show.findIndex((e) => e.itemId === live.itemId);
  if (ei < 0 || live.slideIndex >= s.show[ei]!.slides.length) return null;
  return [ei, live.slideIndex];
}

function after(s: PresentationState, pos: Pos | null): Pos | null {
  if (!pos) {
    const i = s.show.findIndex((e) => e.slides.length > 0);
    return i < 0 ? null : [i, 0];
  }
  const [ei, si] = pos;
  if (si + 1 < s.show[ei]!.slides.length) return [ei, si + 1];
  for (let i = ei + 1; i < s.show.length; i++) if (s.show[i]!.slides.length > 0) return [i, 0];
  return null;
}

function before(s: PresentationState, pos: Pos | null): Pos | null {
  if (!pos) return null;
  const [ei, si] = pos;
  if (si > 0) return [ei, si - 1];
  for (let i = ei - 1; i >= 0; i--) {
    const n = s.show[i]!.slides.length;
    if (n > 0) return [i, n - 1];
  }
  return null;
}

function goLive(s: PresentationState, pos: Pos): PresentationState {
  const nxt = after(s, pos);
  return {
    ...s,
    live: cueAt(s, pos),
    preview: nxt ? cueAt(s, nxt) : null,
    clear: false,
    logo: false,
  };
}

export function mockApply(state: PresentationState, cmd: Command): MockResult {
  const done = (next: PresentationState): MockResult => {
    // Same idempotency rule as Rust: unchanged state keeps its version.
    const { version: _v, ...a } = next;
    const { version: _w, ...b } = state;
    if (JSON.stringify(a) === JSON.stringify(b)) return { ok: true, state };
    return { ok: true, state: { ...next, version: state.version + 1 } };
  };
  const fail = (code: string, message: string): MockResult => ({ ok: false, code, message });

  switch (cmd.kind) {
    case "next":
    case "prev": {
      if (cmd.expectedVersion !== null && cmd.expectedVersion !== state.version) {
        return fail("version_mismatch", `expected ${cmd.expectedVersion}, at ${state.version}`);
      }
      const cur = livePos(state);
      const target = cmd.kind === "next" ? after(state, cur) : before(state, cur);
      return done(target ? goLive(state, target) : state);
    }
    case "goTo": {
      const ei = state.show.findIndex((e) => e.itemId === cmd.itemId);
      if (ei < 0) return fail("unknown_item", cmd.itemId);
      if (cmd.slideIndex >= state.show[ei]!.slides.length) return fail("slide_out_of_range", "");
      const pos: Pos = [ei, cmd.slideIndex];
      return done(
        cmd.target === "live" ? goLive(state, pos) : { ...state, preview: cueAt(state, pos) },
      );
    }
    case "showItem":
      return mockApply(state, {
        kind: "goTo",
        itemId: cmd.itemId,
        slideIndex: 0,
        target: cmd.target,
      });
    case "takePreview": {
      if (!state.preview) return fail("nothing_in_preview", "");
      return done({ ...state, live: state.preview, clear: false, logo: false });
    }
    case "blackout":
      return done({ ...state, blackout: cmd.on });
    case "clear":
      return done({ ...state, clear: cmd.on });
    case "logo":
      return done({ ...state, logo: cmd.on });
    case "serviceMode":
      return done({ ...state, serviceMode: cmd.on });
    case "stageMessage": {
      const t = cmd.text?.trim();
      return done({ ...state, stageMessage: t ? { text: t } : null });
    }
    case "loadShow":
      return done({
        ...state,
        show: cmd.entries,
        showRevision: state.showRevision + 1,
        preview: null,
      });
    default:
      return fail("unsupported", `mock does not model "${cmd.kind}"`);
  }
}
