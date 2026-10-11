import type { ParallelLayout, ServiceItem, SongSummary } from "@hollyshow/protocol";
import { Button, Chip, Field, IconSearch, Modal, TextArea, TextInput } from "@hollyshow/ui";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "../api/context";
import { formatPassage, parseReference } from "../lib/bibleRef";
import { addItem, newItem } from "../lib/playlistOps";
import { useOperator } from "./OperatorContext";
import { useBibleSelection } from "./useBibleSelection";

type Tab = "song" | "bible" | "text" | "countdown" | "header";
const TABS: Tab[] = ["song", "bible", "text", "countdown", "header"];

/** Adds an item to the service: song, Bible passage, text, countdown or section header. */
export function AddItemDialog({
  onClose,
  initialTab = "song",
}: {
  onClose: () => void;
  initialTab?: Tab;
}) {
  const { t, i18n } = useTranslation();
  const api = useApi();
  const { editPlaylist, selectItem, versions } = useOperator();
  const [tab, setTab] = useState<Tab>(initialTab);
  const [query, setQuery] = useState("");
  const [songs, setSongs] = useState<SongSummary[]>([]);
  const [ref, setRef] = useState("");
  const [selected, setSelected] = useBibleSelection(versions);
  const [layout] = useState<ParallelLayout>("columns");
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [minutes, setMinutes] = useState("5");

  useEffect(() => {
    if (tab !== "song") return;
    let alive = true;
    const id = setTimeout(() => {
      void api
        .query({ kind: "searchSongs", text: query, limit: 30 })
        .then((r) => alive && setSongs(r.items));
    }, 100);
    return () => {
      alive = false;
      clearTimeout(id);
    };
  }, [api, tab, query]);

  const lang = i18n.language.startsWith("en") ? "en" : "es";
  const parsed = useMemo(() => parseReference(ref, lang), [ref, lang]);

  const add = (item: ServiceItem) => {
    editPlaylist((p) => addItem(p, item));
    selectItem(item.id);
    onClose();
  };

  return (
    <Modal
      title={t("service.addItem")}
      closeLabel={t("common.close")}
      onClose={onClose}
      width={620}
    >
      <div role="tablist" aria-label={t("service.addItem")} className="hs-tabs">
        {TABS.map((k) => (
          <button
            key={k}
            role="tab"
            type="button"
            className="hs-tab"
            aria-selected={tab === k}
            onClick={() => setTab(k)}
          >
            {t(`service.kind.${k}`)}
          </button>
        ))}
      </div>

      {tab === "song" && (
        <div role="tabpanel" className="stack">
          <label className="searchbox">
            <IconSearch />
            <input
              aria-label={t("songs.search")}
              placeholder={t("songs.search")}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              data-autofocus
            />
          </label>
          <ul className="picklist" aria-label={t("songs.library")}>
            {songs.map((s) => (
              <li key={s.id}>
                <button
                  type="button"
                  className="picklist__row"
                  onClick={() =>
                    add(
                      newItem(
                        s.title,
                        { kind: "song", songId: s.id, arrangementId: null, transpose: 0 },
                        240,
                      ),
                    )
                  }
                >
                  <span>{s.title}</span>
                  <span className="muted">{[s.author, s.key].filter(Boolean).join(" · ")}</span>
                </button>
              </li>
            ))}
            {songs.length === 0 && <li className="muted pad">{t("songs.noResults")}</li>}
          </ul>
        </div>
      )}

      {tab === "bible" && (
        <div role="tabpanel" className="stack">
          <Field
            label={t("bible.reference")}
            hint={
              parsed
                ? `→ ${formatPassage(parsed.passage, lang)}`
                : ref
                  ? t("bible.invalidReference")
                  : undefined
            }
          >
            {(id, hint) => (
              <TextInput
                id={id}
                aria-describedby={hint}
                mono
                value={ref}
                onChange={(e) => setRef(e.target.value)}
                placeholder="Jn 3:16-18"
                data-autofocus
                aria-invalid={ref !== "" && !parsed}
              />
            )}
          </Field>
          <div
            role="group"
            aria-label={t("bible.versionsOnScreen")}
            style={{ display: "flex", flexWrap: "wrap", gap: 8 }}
          >
            {versions.map((v) => (
              <Chip
                key={v.code}
                pressed={selected.includes(v.code)}
                onClick={() =>
                  setSelected(
                    selected.includes(v.code)
                      ? selected.filter((c) => c !== v.code)
                      : selected.length < 4
                        ? [...selected, v.code]
                        : selected,
                  )
                }
              >
                {v.code}
              </Chip>
            ))}
          </div>
          <div>
            <Button
              variant="primary"
              disabled={!parsed || selected.length === 0}
              onClick={() =>
                parsed &&
                add(
                  newItem(
                    t("service.reading", { ref: formatPassage(parsed.passage, lang) }),
                    { kind: "bible", passage: parsed.passage, versions: selected, layout },
                    120,
                  ),
                )
              }
            >
              {t("service.add")}
            </Button>
          </div>
        </div>
      )}

      {tab === "text" && (
        <div role="tabpanel" className="stack">
          <Field label={t("service.itemTitle")}>
            {(id) => (
              <TextInput
                id={id}
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                data-autofocus
              />
            )}
          </Field>
          <Field label={t("service.body")}>
            {(id) => (
              <TextArea id={id} rows={5} value={body} onChange={(e) => setBody(e.target.value)} />
            )}
          </Field>
          <div>
            <Button
              variant="primary"
              disabled={!title.trim() && !body.trim()}
              onClick={() =>
                add(
                  newItem(
                    title.trim() || body.split("\n")[0]!.trim(),
                    { kind: "text", body: body || title },
                    60,
                  ),
                )
              }
            >
              {t("service.add")}
            </Button>
          </div>
        </div>
      )}

      {tab === "countdown" && (
        <div role="tabpanel" className="stack">
          <Field label={t("service.countdownMinutes")}>
            {(id) => (
              <TextInput
                id={id}
                inputMode="numeric"
                mono
                value={minutes}
                onChange={(e) => setMinutes(e.target.value.replace(/\D/g, ""))}
                data-autofocus
              />
            )}
          </Field>
          <div>
            <Button
              variant="primary"
              disabled={!Number(minutes)}
              onClick={() =>
                add(
                  newItem(
                    t("service.countdownTitle"),
                    { kind: "countdown", seconds: Number(minutes) * 60 },
                    Number(minutes) * 60,
                  ),
                )
              }
            >
              {t("service.add")}
            </Button>
          </div>
        </div>
      )}

      {tab === "header" && (
        <div role="tabpanel" className="stack">
          <Field label={t("service.itemTitle")}>
            {(id) => (
              <TextInput
                id={id}
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                data-autofocus
              />
            )}
          </Field>
          <div>
            <Button
              variant="primary"
              disabled={!title.trim()}
              onClick={() => add(newItem(title.trim(), { kind: "header" }, null))}
            >
              {t("service.add")}
            </Button>
          </div>
        </div>
      )}
    </Modal>
  );
}
