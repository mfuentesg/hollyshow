import type { ParallelLayout, ServiceItem, Song } from "@hollyshow/protocol";
import { Button, Chip, Field, Modal, Select, TextArea, TextInput } from "@hollyshow/ui";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "../api/context";
import { transposeKey } from "../lib/chords";
import { useOperator } from "./OperatorContext";

const LAYOUTS: ParallelLayout[] = ["columns", "stacked", "subtitle"];

/** Edits one service item: title, planned duration, notes, theme and kind specific settings. */
export function ItemEditDialog({ item, onClose }: { item: ServiceItem; onClose: () => void }) {
  const { t } = useTranslation();
  const api = useApi();
  const { editPlaylist, themes, versions } = useOperator();
  const [draft, setDraft] = useState<ServiceItem>(item);
  const [song, setSong] = useState<Song | null>(null);
  const [minutes, setMinutes] = useState(
    item.durationSecs === null ? "" : String(Math.floor(item.durationSecs / 60)),
  );
  const [seconds, setSeconds] = useState(
    item.durationSecs === null ? "" : String(item.durationSecs % 60),
  );

  const songId = item.content.kind === "song" ? item.content.songId : null;
  useEffect(() => {
    if (!songId) return;
    let alive = true;
    void api.query({ kind: "getSong", id: songId }).then((r) => alive && setSong(r.song));
    return () => {
      alive = false;
    };
  }, [api, songId]);

  const save = () => {
    const m =
      minutes.trim() === "" && seconds.trim() === ""
        ? null
        : Math.max(0, Number(minutes) || 0) * 60 + Math.min(59, Math.max(0, Number(seconds) || 0));
    const next: ServiceItem = {
      ...draft,
      title: draft.title.trim() || item.title,
      durationSecs: m && m > 0 ? m : null,
    };
    editPlaylist((p) => ({ ...p, items: p.items.map((i) => (i.id === item.id ? next : i)) }));
    onClose();
  };

  const c = draft.content;
  return (
    <Modal
      title={t("service.editItem")}
      closeLabel={t("common.close")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" onClick={save}>
            {t("common.save")}
          </Button>
        </>
      }
    >
      <Field label={t("service.itemTitle")}>
        {(id) => (
          <TextInput
            id={id}
            value={draft.title}
            onChange={(e) => setDraft({ ...draft, title: e.target.value })}
            data-autofocus
          />
        )}
      </Field>
      <div style={{ display: "flex", gap: 12 }}>
        <Field label={t("service.minutes")} style={{ flex: 1 }}>
          {(id) => (
            <TextInput
              id={id}
              inputMode="numeric"
              mono
              value={minutes}
              onChange={(e) => setMinutes(e.target.value.replace(/\D/g, ""))}
            />
          )}
        </Field>
        <Field label={t("service.seconds")} style={{ flex: 1 }}>
          {(id) => (
            <TextInput
              id={id}
              inputMode="numeric"
              mono
              value={seconds}
              onChange={(e) => setSeconds(e.target.value.replace(/\D/g, ""))}
            />
          )}
        </Field>
      </div>
      <Field label={t("service.theme")}>
        {(id) => (
          <Select
            id={id}
            value={draft.themeId ?? ""}
            onChange={(e) => setDraft({ ...draft, themeId: e.target.value || null })}
          >
            <option value="">{t("service.themeDefault")}</option>
            {themes.map((th) => (
              <option key={th.id} value={th.id}>
                {th.name}
              </option>
            ))}
          </Select>
        )}
      </Field>

      {c.kind === "song" && (
        <>
          <Field label={t("service.arrangement")}>
            {(id) => (
              <Select
                id={id}
                value={c.arrangementId ?? ""}
                onChange={(e) =>
                  setDraft({ ...draft, content: { ...c, arrangementId: e.target.value || null } })
                }
              >
                <option value="">{t("service.arrangementDefault")}</option>
                {song?.arrangements.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field
            label={t("service.transpose")}
            hint={
              song?.key ? `${song.key} → ${transposeKey(song.key, c.transpose) ?? "—"}` : undefined
            }
          >
            {(id) => (
              <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                <Button
                  size="sm"
                  aria-label={t("service.transposeDown")}
                  onClick={() =>
                    setDraft({
                      ...draft,
                      content: { ...c, transpose: Math.max(-11, c.transpose - 1) },
                    })
                  }
                >
                  −
                </Button>
                <output id={id} className="mono" style={{ minWidth: 40, textAlign: "center" }}>
                  {c.transpose > 0 ? `+${c.transpose}` : c.transpose}
                </output>
                <Button
                  size="sm"
                  aria-label={t("service.transposeUp")}
                  onClick={() =>
                    setDraft({
                      ...draft,
                      content: { ...c, transpose: Math.min(11, c.transpose + 1) },
                    })
                  }
                >
                  +
                </Button>
              </div>
            )}
          </Field>
        </>
      )}

      {c.kind === "bible" && (
        <>
          <div
            role="group"
            aria-label={t("bible.versionsOnScreen")}
            style={{ display: "flex", flexWrap: "wrap", gap: 8 }}
          >
            {versions.map((v) => {
              const on = c.versions.includes(v.code);
              return (
                <Chip
                  key={v.code}
                  pressed={on}
                  onClick={() => {
                    const list = on
                      ? c.versions.filter((x) => x !== v.code)
                      : c.versions.length < 4
                        ? [...c.versions, v.code]
                        : c.versions;
                    setDraft({ ...draft, content: { ...c, versions: list } });
                  }}
                >
                  {on ? `${c.versions.indexOf(v.code) + 1} · ` : ""}
                  {v.code}
                </Chip>
              );
            })}
          </div>
          <Field label={t("bible.layout")}>
            {(id) => (
              <Select
                id={id}
                value={c.layout}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    content: { ...c, layout: e.target.value as ParallelLayout },
                  })
                }
              >
                {LAYOUTS.map((l) => (
                  <option key={l} value={l}>
                    {t(`bible.layouts.${l}`)}
                  </option>
                ))}
              </Select>
            )}
          </Field>
        </>
      )}

      {c.kind === "text" && (
        <Field label={t("service.body")}>
          {(id) => (
            <TextArea
              id={id}
              rows={5}
              value={c.body}
              onChange={(e) => setDraft({ ...draft, content: { ...c, body: e.target.value } })}
            />
          )}
        </Field>
      )}

      {c.kind === "countdown" && (
        <Field label={t("service.countdownMinutes")}>
          {(id) => (
            <TextInput
              id={id}
              inputMode="numeric"
              mono
              value={String(Math.round(c.seconds / 60))}
              onChange={(e) =>
                setDraft({
                  ...draft,
                  content: { ...c, seconds: (Number(e.target.value.replace(/\D/g, "")) || 0) * 60 },
                })
              }
            />
          )}
        </Field>
      )}

      <Field label={t("service.notes")}>
        {(id) => (
          <TextArea
            id={id}
            rows={3}
            value={draft.notes ?? ""}
            onChange={(e) => setDraft({ ...draft, notes: e.target.value || null })}
          />
        )}
      </Field>
    </Modal>
  );
}
