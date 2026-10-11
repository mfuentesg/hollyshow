import { useEffect, useRef } from "react";
import { useLocation } from "react-router-dom";
import { cmd } from "../lib/cmd";
import type { KeyBuffer } from "../lib/sections";
import { findSectionSlide } from "../lib/sections";
import { decideKey, targetKind, type ShortcutAction } from "../lib/shortcuts";
import type { OperatorCtx } from "./OperatorContext";
import { selectedEntry } from "./selection";

const ANYWHERE = new Set<ShortcutAction["type"]>(["palette", "blackout", "logo", "clear", "help"]);

/**
 * Window-level keyboard map. Panic keys (B / L / X), the palette and help work on every screen;
 * slide navigation, Enter and section jumps only on the service screen, where nothing else needs
 * the arrow keys.
 */
export function useOperatorKeys(op: OperatorCtx, openHelp: () => void) {
  const loc = useLocation();
  const opRef = useRef(op);
  opRef.current = op;
  const helpRef = useRef(openHelp);
  helpRef.current = openHelp;
  const onService = loc.pathname.replace(/\/$/, "") === "/operator";
  const buffer = useRef<KeyBuffer>({ buffer: "", at: 0 });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      const r = decideKey(
        {
          key: e.key,
          ctrl: e.ctrlKey,
          meta: e.metaKey,
          shift: e.shiftKey,
          alt: e.altKey,
          target: targetKind(e.target),
        },
        buffer.current,
        Date.now(),
      );
      buffer.current = r.buffer;
      const a = r.action;
      if (!a) return;
      if (!onService && !ANYWHERE.has(a.type)) return;
      if (r.consume) e.preventDefault();
      const o = opRef.current;
      const view = o.p.view;
      switch (a.type) {
        case "next":
          void o.send(cmd.next(view.version));
          break;
        case "prev":
          void o.send(cmd.prev(view.version));
          break;
        case "takePreview":
          if (view.preview) void o.send(cmd.takePreview());
          break;
        case "blackout":
          void o.send(cmd.blackout(!view.blackout));
          break;
        case "logo":
          void o.send(cmd.logo(!view.logo));
          break;
        case "clear":
          void o.send(cmd.clear(!view.clear));
          break;
        case "undo":
          o.undo();
          break;
        case "redo":
          o.redo();
          break;
        case "palette":
          o.setPaletteOpen(true);
          break;
        case "help":
          helpRef.current();
          break;
        case "jump": {
          const entry = selectedEntry(o);
          if (!entry) break;
          const idx = findSectionSlide(entry.slides, a.section);
          if (idx >= 0) void o.send(cmd.goTo(entry.itemId, idx, a.to));
          break;
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onService]);
}
