import type { BibleHit, SongSummary } from "@hollyshow/protocol";
import { IconSearch, Modal } from "@hollyshow/ui";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { useApi } from "../api/context";
import { formatPassage, parseReference, shortPassage } from "../lib/bibleRef";
import { cmd } from "../lib/cmd";
import { addItem, newItem } from "../lib/playlistOps";
import { matchesAll } from "../lib/text";
import { useOperator } from "./OperatorContext";
import { useCheckForUpdatesEntry } from "./update";
import { useBibleSelection } from "./useBibleSelection";

type Mode = "primary" | "live" | "alt";

interface Entry {
  id: string;
  group: "actions" | "songs" | "bible" | "service";
  label: string;
  hint?: string;
  /** What Ctrl+Enter / Alt+Enter do, shown next to the row. */
  modes?: string;
  run: (mode: Mode) => void | Promise<void>;
}

export interface PaletteProps {
  onClose: () => void;
  onOpenTemplates: () => void;
  onOpenHelp: () => void;
}

/** Cmd/Ctrl+K: songs, Bible references (and verse text) and actions in one box. */
export function CommandPalette({ onClose, onOpenTemplates, onOpenHelp }: PaletteProps) {
  const { t, i18n } = useTranslation();
  const api = useApi();
  const nav = useNavigate();
  const op = useOperator();
  const { p, playlist, versions } = op;
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [songs, setSongs] = useState<SongSummary[]>([]);
  const [hits, setHits] = useState<BibleHit[]>([]);
  const [selection] = useBibleSelection(versions);
  const listId = useId();
  const updateEntry = useCheckForUpdatesEntry();
  const lang = i18n.language.startsWith("en") ? "en" : "es";
  const q = query.trim();
  const ref = useMemo(() => (q ? parseReference(q, lang) : null), [q, lang]);

  useEffect(() => {
    let alive = true;
    const id = setTimeout(() => {
      void api
        .query({ kind: "searchSongs", text: q, limit: q ? 6 : 4 })
        .then((r) => alive && setSongs(r.items));
      if (q.length >= 3 && !ref) {
        void api
          .query({ kind: "searchBible", text: q, version: selection[0] ?? null, limit: 4 })
          .then((r) => alive && setHits(r.items));
      } else setHits([]);
    }, 120);
    return () => {
      alive = false;
      clearTimeout(id);
    };
  }, [api, q, ref, selection]);

  const entries = useMemo<Entry[]>(() => {
    const out: Entry[] = [];
    const go = (to: string) => () => nav(to);
    const versionsLabel = selection.join(" + ");

    if (ref) {
      const label = formatPassage(ref.passage, lang);
      const showRef = (target: "preview" | "live") =>
        op.send(cmd.showBible(ref.passage, selection, target));
      out.push({
        id: "ref",
        group: "bible",
        label,
        hint: versionsLabel,
        modes: t("palette.refModes"),
        run: async (mode) => {
          if (mode === "alt") {
            if (!playlist) return;
            op.editPlaylist((pl) =>
              addItem(
                pl,
                newItem(
                  t("service.reading", { ref: label }),
                  { kind: "bible", passage: ref.passage, versions: selection, layout: "columns" },
                  120,
                ),
              ),
            );
            return;
          }
          await showRef(mode === "live" ? "live" : "preview");
        },
      });
      out.push({
        id: "ref-open",
        group: "bible",
        label: t("palette.openInBible", { ref: label }),
        run: () =>
          nav(`/operator/bible?ref=${encodeURIComponent(shortPassage(ref.passage, lang))}`),
      });
    }

    for (const s of songs) {
      out.push({
        id: `song:${s.id}`,
        group: "songs",
        label: s.title,
        hint: [s.author, s.key, s.snippet ? `«…${s.snippet}…»` : null].filter(Boolean).join(" · "),
        modes: t("palette.songModes"),
        run: (mode) => {
          if (mode === "alt") return nav(`/operator/songs/${encodeURIComponent(s.id)}`);
          if (!playlist) return;
          const item = newItem(
            s.title,
            { kind: "song", songId: s.id, arrangementId: null, transpose: 0 },
            240,
          );
          op.editPlaylist((pl) => addItem(pl, item));
          op.selectItem(item.id);
        },
      });
    }

    for (const h of hits) {
      const passage = { start: h.reference, end: h.reference };
      out.push({
        id: `hit:${h.version}:${h.reference.book}.${h.reference.chapter}.${h.reference.verse}`,
        group: "bible",
        label: `${formatPassage(passage, lang)} · ${h.version}`,
        hint: h.text,
        modes: t("palette.refModes"),
        run: async (mode) => {
          if (mode === "alt") {
            if (playlist)
              op.editPlaylist((pl) =>
                addItem(
                  pl,
                  newItem(
                    formatPassage(passage, lang),
                    { kind: "bible", passage, versions: selection, layout: "columns" },
                    60,
                  ),
                ),
              );
            return;
          }
          await op.send(cmd.showBible(passage, selection, mode === "live" ? "live" : "preview"));
        },
      });
    }

    if (q) {
      for (const it of playlist?.items ?? []) {
        if (it.content.kind === "header" || !matchesAll(it.title, q)) continue;
        out.push({
          id: `item:${it.id}`,
          group: "service",
          label: it.title,
          modes: t("palette.itemModes"),
          run: async (mode) => {
            op.selectItem(it.id);
            await op.send(cmd.showItem(it.id, mode === "live" ? "live" : "preview"));
          },
        });
      }
    }

    const view = p.view;
    const actions: Entry[] = [
      {
        id: "a:next",
        group: "actions",
        label: t("operator.next"),
        hint: "→",
        run: () => void op.send(cmd.next(view)),
      },
      {
        id: "a:prev",
        group: "actions",
        label: t("operator.prev"),
        hint: "←",
        run: () => void op.send(cmd.prev(view)),
      },
      {
        id: "a:take",
        group: "actions",
        label: t("operator.takePreview"),
        hint: "Enter",
        run: () => void op.send(cmd.takePreview()),
      },
      {
        id: "a:black",
        group: "actions",
        label: view.blackout ? t("palette.blackoutOff") : t("palette.blackoutOn"),
        hint: "B",
        run: () => void op.send(cmd.blackout(!view.blackout)),
      },
      {
        id: "a:clear",
        group: "actions",
        label: view.clear ? t("palette.clearOff") : t("palette.clearOn"),
        hint: "X",
        run: () => void op.send(cmd.clear(!view.clear)),
      },
      {
        id: "a:logo",
        group: "actions",
        label: view.logo ? t("palette.logoOff") : t("palette.logoOn"),
        hint: "L",
        run: () => void op.send(cmd.logo(!view.logo)),
      },
      {
        id: "a:mode",
        group: "actions",
        label: view.serviceMode ? t("palette.serviceModeOff") : t("palette.serviceModeOn"),
        run: () => void op.send(cmd.serviceMode(!view.serviceMode)),
      },
      { id: "a:undo", group: "actions", label: t("common.undo"), hint: "Ctrl+Z", run: op.undo },
      {
        id: "a:redo",
        group: "actions",
        label: t("common.redo"),
        hint: "Ctrl+Shift+Z",
        run: op.redo,
      },
      { id: "a:service", group: "actions", label: t("palette.goService"), run: go("/operator") },
      { id: "a:songs", group: "actions", label: t("palette.goSongs"), run: go("/operator/songs") },
      {
        id: "a:newsong",
        group: "actions",
        label: t("palette.newSong"),
        run: go("/operator/songs/new"),
      },
      { id: "a:bible", group: "actions", label: t("palette.goBible"), run: go("/operator/bible") },
      {
        id: "a:themes",
        group: "actions",
        label: t("palette.goThemes"),
        run: go("/operator/themes"),
      },
      {
        id: "a:outputs",
        group: "actions",
        label: t("palette.goOutputs"),
        run: go("/operator/settings/outputs"),
      },
      {
        id: "a:remote",
        group: "actions",
        label: t("palette.goRemote"),
        run: go("/operator/settings/remote"),
      },
      {
        id: "a:versions",
        group: "actions",
        label: t("palette.goVersions"),
        run: go("/operator/settings/bible"),
      },
      {
        id: "a:lang",
        group: "actions",
        label: t("palette.goLanguage"),
        run: go("/operator/settings/language"),
      },
      { id: "a:templates", group: "actions", label: t("palette.templates"), run: onOpenTemplates },
      ...(updateEntry.disabled
        ? []
        : [
            {
              id: updateEntry.id,
              group: "actions" as const,
              label: updateEntry.label,
              run: () => void updateEntry.run(),
            },
          ]),
      { id: "a:help", group: "actions", label: t("shortcuts.title"), hint: "?", run: onOpenHelp },
    ];
    const shownActions = q ? actions.filter((a) => matchesAll(a.label, q)) : actions.slice(0, 9);
    return [
      ...out.filter((e) => e.group !== "service"),
      ...out.filter((e) => e.group === "service"),
      ...shownActions,
    ];
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ref, songs, hits, q, p.view, playlist, selection, lang, op.send, t, updateEntry]);

  useEffect(() => setActive(0), [q]);
  const activeIdx = Math.min(active, Math.max(0, entries.length - 1));
  const listRef = useRef<HTMLUListElement>(null);
  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>("[aria-selected=true]")
      ?.scrollIntoView?.({ block: "nearest" });
  }, [activeIdx]);

  const run = (e: Entry | undefined, mode: Mode) => {
    if (!e) return;
    onClose();
    void e.run(mode);
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(entries.length - 1, i + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(0, i - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(entries[activeIdx], e.ctrlKey || e.metaKey ? "live" : e.altKey ? "alt" : "primary");
    }
  };

  const groups: Entry["group"][] = ["bible", "songs", "service", "actions"];
  let n = -1;
  return (
    <Modal
      title={t("palette.title")}
      closeLabel={t("common.close")}
      onClose={onClose}
      bare
      width={640}
    >
      <div className="palette">
        <label className="palette__input">
          <IconSearch size={18} />
          <input
            data-autofocus
            role="combobox"
            aria-expanded="true"
            aria-controls={listId}
            aria-activedescendant={entries[activeIdx] ? `${listId}-${activeIdx}` : undefined}
            aria-autocomplete="list"
            aria-label={t("palette.title")}
            placeholder={t("palette.placeholder")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKey}
            autoComplete="off"
            spellCheck={false}
          />
        </label>
        <ul
          id={listId}
          role="listbox"
          aria-label={t("palette.results")}
          ref={listRef}
          className="palette__list"
        >
          {groups.map((g) => {
            const rows = entries.filter((e) => e.group === g);
            if (rows.length === 0) return null;
            return (
              <li key={g} role="presentation">
                <div className="palette__group" role="presentation">
                  {t(`palette.groups.${g}`)}
                </div>
                <ul role="presentation" className="palette__sub">
                  {rows.map((e) => {
                    n += 1;
                    const idx = n;
                    return (
                      <li
                        key={e.id}
                        id={`${listId}-${idx}`}
                        role="option"
                        aria-selected={idx === activeIdx}
                        className={`palette__row${idx === activeIdx ? " is-active" : ""}`}
                        onMouseMove={() => setActive(idx)}
                        onClick={(ev) =>
                          run(e, ev.ctrlKey || ev.metaKey ? "live" : ev.altKey ? "alt" : "primary")
                        }
                      >
                        <span className="palette__label">{e.label}</span>
                        {e.hint && <span className="palette__hint">{e.hint}</span>}
                        {idx === activeIdx && e.modes && (
                          <span className="palette__modes">{e.modes}</span>
                        )}
                      </li>
                    );
                  })}
                </ul>
              </li>
            );
          })}
          {entries.length === 0 && (
            <li className="muted pad" role="presentation">
              {t("palette.noResults")}
            </li>
          )}
        </ul>
        <div className="palette__foot muted small">
          <span>↑↓ {t("palette.navigate")}</span>
          <span>Enter {t("palette.run")}</span>
          <span>Esc {t("common.close")}</span>
        </div>
      </div>
    </Modal>
  );
}
