import type { ItemContent, Playlist, ServiceItem } from "@hollyshow/protocol";
import { uid } from "./text";

/** Pure, immutable playlist edits. The host persists the result; the UI keeps undo history. */

export function newItem(
  title: string,
  content: ItemContent,
  durationSecs: number | null = null,
): ServiceItem {
  return { id: uid("item"), title, content, durationSecs, themeId: null, notes: null };
}

export function addItem(p: Playlist, item: ServiceItem, index?: number): Playlist {
  const items = [...p.items];
  items.splice(
    index === undefined ? items.length : Math.min(Math.max(index, 0), items.length),
    0,
    item,
  );
  return { ...p, items };
}

export function removeItem(p: Playlist, id: string): Playlist {
  return { ...p, items: p.items.filter((i) => i.id !== id) };
}

export function updateItem(p: Playlist, id: string, patch: Partial<ServiceItem>): Playlist {
  return { ...p, items: p.items.map((i) => (i.id === id ? { ...i, ...patch, id: i.id } : i)) };
}

export function duplicateItem(p: Playlist, id: string): Playlist {
  const idx = p.items.findIndex((i) => i.id === id);
  if (idx < 0) return p;
  const copy: ServiceItem = { ...p.items[idx]!, id: uid("item") };
  return addItem(p, copy, idx + 1);
}

/** Moves the item at `from` so it ends up at index `to` (drag and drop, Alt+arrows). */
export function moveItem(p: Playlist, from: number, to: number): Playlist {
  if (from === to || from < 0 || from >= p.items.length) return p;
  const target = Math.min(Math.max(to, 0), p.items.length - 1);
  if (target === from) return p;
  const items = [...p.items];
  const [it] = items.splice(from, 1);
  items.splice(target, 0, it!);
  return { ...p, items };
}

/** Template copy: new id, no date, flagged as template. */
export function toTemplate(p: Playlist, name: string): Playlist {
  return {
    ...p,
    id: uid("tpl"),
    name,
    date: null,
    isTemplate: true,
    items: p.items.map((i) => ({ ...i, id: uid("item") })),
  };
}

/** Service from a template: fresh ids so the template is never mutated. */
export function instantiate(template: Playlist, name: string, date: string | null): Playlist {
  return {
    ...template,
    id: uid("svc"),
    name,
    date,
    isTemplate: false,
    items: template.items.map((i) => ({ ...i, id: uid("item") })),
  };
}

export function itemKindLabelKey(c: ItemContent): string {
  switch (c.kind) {
    case "song":
      return "service.kind.song";
    case "bible":
      return "service.kind.bible";
    case "media":
      return "service.kind.media";
    case "text":
      return "service.kind.text";
    case "countdown":
      return "service.kind.countdown";
    case "header":
      return "service.kind.header";
  }
}
