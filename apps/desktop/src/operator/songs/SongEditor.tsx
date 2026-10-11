import type { Song } from "@hollyshow/protocol";
import { SlideView } from "@hollyshow/slide-renderer";
import {
  Button,
  Chip,
  Field,
  IconButton,
  IconCopy,
  IconDown,
  IconRedo,
  IconTrash,
  IconUndo,
  IconUp,
  Modal,
  Select,
  TextArea,
  TextInput,
} from "@hollyshow/ui";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { useHost } from "../../api/context";
import { useToasts } from "../../hooks/useToasts";
import { transposeKey, transposeText } from "../../lib/chords";
import { useHistory } from "../../lib/history";
import {
  addArrangement,
  addSection,
  duplicateSection,
  moveInSequence,
  moveSection,
  patchSong,
  removeArrangement,
  removeSection,
  songSlides,
  updateArrangement,
  updateSection,
} from "../../lib/songOps";
import { SECTION_KINDS, countSlides, sectionCodes, suggestSections } from "../../lib/songText";
import { useOperator } from "../OperatorContext";

function TextToSections({
  onApply,
  onClose,
}: {
  onApply: (text: string) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  return (
    <Modal
      title={t("songs.splitText")}
      closeLabel={t("common.close")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" disabled={!text.trim()} onClick={() => onApply(text)}>
            {t("songs.splitApply")}
          </Button>
        </>
      }
    >
      <Field label={t("songs.splitLabel")} hint={t("songs.splitHint")}>
        {(id, d) => (
          <TextArea
            id={id}
            aria-describedby={d}
            rows={12}
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        )}
      </Field>
    </Modal>
  );
}

/** Editor for one song (docs/design/SongEditor.dc.html): fields, sections, arrangement chips, preview. */
export function SongEditor({ initial, isNew }: { initial: Song; isNew: boolean }) {
  const { t } = useTranslation();
  const host = useHost();
  const nav = useNavigate();
  const op = useOperator();
  const toasts = useToasts();
  const h = useHistory<Song>(initial);
  const [saved, setSaved] = useState<Song>(isNew ? { ...initial, id: "" } : initial);
  const [arrId, setArrId] = useState<string | null>(initial.arrangements[0]?.id ?? null);
  const [dragChip, setDragChip] = useState<number | null>(null);
  const [dialog, setDialog] = useState<null | "delete" | "split">(null);
  const [saving, setSaving] = useState(false);
  const song = h.value;
  const dirty = isNew || JSON.stringify(song) !== JSON.stringify(saved);
  const codes = useMemo(() => sectionCodes(song.sections), [song.sections]);
  const theme = op.themeFor(song.themeId);
  const arrangement = song.arrangements.find((a) => a.id === arrId) ?? song.arrangements[0];
  const preview = useMemo(
    () =>
      songSlides(song, {
        arrangementId: arrangement?.id ?? null,
        maxLines: theme.maxLinesPerSlide,
        copyright: "last",
        idPrefix: "prev",
      }),
    [song, arrangement?.id, theme.maxLinesPerSlide],
  );

  const edit = (next: Song, key?: string) => h.set(next, key);

  const save = async () => {
    if (!song.title.trim()) {
      toasts.push(t("songs.titleRequired"), "error");
      return;
    }
    setSaving(true);
    try {
      const out = await host.saveSong({ ...song, title: song.title.trim() });
      h.reset(out);
      setSaved(out);
      await op.reloadLibrary();
      toasts.push(t("songs.savedToast", { title: out.title }), "success");
      if (isNew) nav(`/operator/songs/${encodeURIComponent(out.id)}`, { replace: true });
    } catch (e) {
      toasts.error(e);
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    try {
      await host.deleteSong(song.id);
      await op.reloadLibrary();
      nav("/operator/songs", { replace: true });
    } catch (e) {
      toasts.error(e);
    }
  };

  const transpose = (n: number) =>
    edit({
      ...song,
      key: transposeKey(song.key, n),
      sections: song.sections.map((s) => ({ ...s, lyrics: transposeText(s.lyrics, n, n < 0) })),
    });

  return (
    <div
      className="song-editor"
      onKeyDown={(e) => {
        const mod = e.ctrlKey || e.metaKey;
        if (!mod || e.altKey) return;
        const k = e.key.toLowerCase();
        if (k === "z") {
          e.preventDefault();
          if (e.shiftKey) h.redo();
          else h.undo();
        } else if (k === "y") {
          e.preventDefault();
          h.redo();
        } else if (k === "s") {
          e.preventDefault();
          void save();
        }
      }}
    >
      <div className="row gap-8 wrap song-editor__bar">
        <h1 className="grow">{isNew ? t("songs.newSong") : song.title || t("songs.untitled")}</h1>
        <IconButton aria-label={t("common.undo")} disabled={!h.canUndo} onClick={() => h.undo()}>
          <IconUndo />
        </IconButton>
        <IconButton aria-label={t("common.redo")} disabled={!h.canRedo} onClick={() => h.redo()}>
          <IconRedo />
        </IconButton>
        <Button size="sm" onClick={() => transpose(-1)} aria-label={t("songs.transposeDown")}>
          −1
        </Button>
        <Button size="sm" onClick={() => transpose(1)} aria-label={t("songs.transposeUp")}>
          +1
        </Button>
        <Button size="sm" onClick={() => setDialog("split")}>
          {t("songs.splitText")}
        </Button>
        {!isNew && (
          <Button size="sm" variant="danger" onClick={() => setDialog("delete")}>
            {t("common.delete")}
          </Button>
        )}
        <Button variant="primary" disabled={!dirty || saving} onClick={() => void save()}>
          {dirty ? t("common.save") : t("songs.upToDate")}
        </Button>
      </div>

      <div className="song-fields">
        <Field label={t("songs.title")} className="span-2">
          {(id) => (
            <TextInput
              id={id}
              value={song.title}
              onChange={(e) => edit(patchSong(song, { title: e.target.value }), "title")}
            />
          )}
        </Field>
        <Field label={t("songs.key")}>
          {(id) => (
            <TextInput
              id={id}
              mono
              value={song.key ?? ""}
              onChange={(e) => edit(patchSong(song, { key: e.target.value || null }), "key")}
            />
          )}
        </Field>
        <Field label={t("songs.copyright")}>
          {(id) => (
            <TextInput
              id={id}
              value={song.copyright ?? ""}
              onChange={(e) =>
                edit(patchSong(song, { copyright: e.target.value || null }), "copyright")
              }
            />
          )}
        </Field>
        <Field label={t("songs.author")} className="span-2">
          {(id) => (
            <TextInput
              id={id}
              value={song.author ?? ""}
              onChange={(e) => edit(patchSong(song, { author: e.target.value || null }), "author")}
            />
          )}
        </Field>
        <Field label={t("songs.ccli")}>
          {(id) => (
            <TextInput
              id={id}
              mono
              value={song.ccli ?? ""}
              onChange={(e) => edit(patchSong(song, { ccli: e.target.value || null }), "ccli")}
            />
          )}
        </Field>
        <Field label={t("songs.theme")}>
          {(id) => (
            <Select
              id={id}
              value={song.themeId ?? ""}
              onChange={(e) => edit(patchSong(song, { themeId: e.target.value || null }))}
            >
              <option value="">{t("service.themeDefault")}</option>
              {op.themes.map((th) => (
                <option key={th.id} value={th.id}>
                  {th.name}
                </option>
              ))}
            </Select>
          )}
        </Field>
      </div>

      <section className="stack" aria-labelledby="arr-h">
        <div className="row gap-8 wrap">
          <h2 id="arr-h" className="subhead grow">
            {t("songs.arrangement")}
          </h2>
          <Select
            aria-label={t("songs.arrangementPick")}
            value={arrangement?.id ?? ""}
            onChange={(e) => setArrId(e.target.value)}
            disabled={song.arrangements.length === 0}
          >
            {song.arrangements.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </Select>
          <Button
            size="sm"
            onClick={() => {
              const r = addArrangement(
                song,
                t("songs.arrangementName", { n: song.arrangements.length + 1 }),
              );
              edit(r.song);
              setArrId(r.arrangement.id);
            }}
          >
            + {t("songs.newArrangement")}
          </Button>
        </div>
        {arrangement ? (
          <>
            <div className="row gap-8">
              <TextInput
                aria-label={t("songs.arrangementNameLabel")}
                value={arrangement.name}
                onChange={(e) =>
                  edit(
                    updateArrangement(song, arrangement.id, { name: e.target.value }),
                    `arr-${arrangement.id}`,
                  )
                }
              />
              <Button
                size="sm"
                variant="ghost"
                disabled={song.arrangements.length <= 1}
                onClick={() => {
                  edit(removeArrangement(song, arrangement.id));
                  setArrId(null);
                }}
              >
                {t("common.delete")}
              </Button>
            </div>
            <ul className="chips" aria-label={t("songs.sequence")}>
              {arrangement.sequence.map((sid, i) => {
                const sec = song.sections.find((s) => s.id === sid);
                return (
                  <li
                    key={`${sid}-${i}`}
                    className={`seq-chip${dragChip === i ? " is-drag" : ""}`}
                    draggable
                    onDragStart={(e) => {
                      e.dataTransfer.effectAllowed = "move";
                      e.dataTransfer.setData("text/plain", String(i));
                      setDragChip(i);
                    }}
                    onDragOver={(e) => dragChip !== null && e.preventDefault()}
                    onDrop={(e) => {
                      e.preventDefault();
                      if (dragChip !== null)
                        edit(
                          updateArrangement(song, arrangement.id, {
                            sequence: moveInSequence(arrangement.sequence, dragChip, i),
                          }),
                        );
                      setDragChip(null);
                    }}
                    onDragEnd={() => setDragChip(null)}
                  >
                    <span className="mono" title={sec?.label}>
                      {codes.get(sid) ?? "?"}
                    </span>
                    <button
                      type="button"
                      className="seq-chip__btn"
                      aria-label={t("songs.chipLeft", { label: sec?.label ?? "" })}
                      disabled={i === 0}
                      onClick={() =>
                        edit(
                          updateArrangement(song, arrangement.id, {
                            sequence: moveInSequence(arrangement.sequence, i, i - 1),
                          }),
                        )
                      }
                    >
                      ‹
                    </button>
                    <button
                      type="button"
                      className="seq-chip__btn"
                      aria-label={t("songs.chipRight", { label: sec?.label ?? "" })}
                      disabled={i === arrangement.sequence.length - 1}
                      onClick={() =>
                        edit(
                          updateArrangement(song, arrangement.id, {
                            sequence: moveInSequence(arrangement.sequence, i, i + 1),
                          }),
                        )
                      }
                    >
                      ›
                    </button>
                    <button
                      type="button"
                      className="seq-chip__btn"
                      aria-label={t("songs.chipRemove", { label: sec?.label ?? "" })}
                      onClick={() =>
                        edit(
                          updateArrangement(song, arrangement.id, {
                            sequence: arrangement.sequence.filter((_, j) => j !== i),
                          }),
                        )
                      }
                    >
                      ×
                    </button>
                  </li>
                );
              })}
              {song.sections.map((s) => (
                <li key={`add-${s.id}`}>
                  <Chip
                    tone="dashed"
                    aria-label={t("songs.chipAdd", { label: s.label })}
                    onClick={() =>
                      edit(
                        updateArrangement(song, arrangement.id, {
                          sequence: [...arrangement.sequence, s.id],
                        }),
                      )
                    }
                  >
                    + {codes.get(s.id)}
                  </Chip>
                </li>
              ))}
            </ul>
          </>
        ) : (
          <p className="muted">{t("songs.noArrangement")}</p>
        )}
      </section>

      <div className="song-split">
        <section className="stack" aria-labelledby="sec-h">
          <div className="row gap-8 wrap">
            <h2 id="sec-h" className="subhead grow">
              {t("songs.sections")}
            </h2>
            {SECTION_KINDS.slice(0, 4).map((k) => (
              <Button key={k} size="sm" onClick={() => edit(addSection(song, k).song)}>
                + {t(`songs.kinds.${k}`)}
              </Button>
            ))}
          </div>
          <p className="muted small">{t("songs.breakHint")}</p>
          {song.sections.map((s, i) => (
            <div key={s.id} className="sec-card" role="group" aria-label={s.label}>
              <div className="row gap-8 wrap">
                <span className="mono sec-card__code">{codes.get(s.id)}</span>
                <TextInput
                  aria-label={t("songs.sectionLabel")}
                  value={s.label}
                  onChange={(e) =>
                    edit(updateSection(song, s.id, { label: e.target.value }), `l-${s.id}`)
                  }
                  style={{ flex: 1, minWidth: 120 }}
                />
                <Select
                  aria-label={t("songs.sectionKind")}
                  value={s.kind}
                  onChange={(e) =>
                    edit(updateSection(song, s.id, { kind: e.target.value as typeof s.kind }))
                  }
                >
                  {SECTION_KINDS.map((k) => (
                    <option key={k} value={k}>
                      {t(`songs.kinds.${k}`)}
                    </option>
                  ))}
                </Select>
                <IconButton
                  aria-label={t("songs.sectionUp", { label: s.label })}
                  disabled={i === 0}
                  onClick={() => edit(moveSection(song, i, i - 1))}
                >
                  <IconUp size={14} />
                </IconButton>
                <IconButton
                  aria-label={t("songs.sectionDown", { label: s.label })}
                  disabled={i === song.sections.length - 1}
                  onClick={() => edit(moveSection(song, i, i + 1))}
                >
                  <IconDown size={14} />
                </IconButton>
                <IconButton
                  aria-label={t("songs.sectionDup", { label: s.label })}
                  onClick={() => edit(duplicateSection(song, s.id))}
                >
                  <IconCopy size={14} />
                </IconButton>
                <IconButton
                  aria-label={t("songs.sectionRemove", { label: s.label })}
                  disabled={song.sections.length <= 1}
                  onClick={() => edit(removeSection(song, s.id))}
                >
                  <IconTrash size={14} />
                </IconButton>
              </div>
              <TextArea
                aria-label={t("songs.lyricsOf", { label: s.label })}
                className="lyrics"
                rows={Math.max(4, s.lyrics.split("\n").length + 1)}
                spellCheck
                value={s.lyrics}
                onChange={(e) =>
                  edit(updateSection(song, s.id, { lyrics: e.target.value }), `ly-${s.id}`)
                }
              />
              <span className="muted small">
                {t("songs.slideCount", { count: countSlides(s.lyrics, theme.maxLinesPerSlide) })}
              </span>
            </div>
          ))}
        </section>

        <section className="stack" aria-labelledby="prev-h">
          <h2 id="prev-h" className="subhead">
            {t("songs.preview")} · {t("songs.slideCount", { count: preview.length })}
          </h2>
          <ul className="preview-list">
            {preview.map((s) => (
              <li key={s.id} className="preview-tile">
                <div className="preview-tile__screen">
                  <SlideView slide={s} theme={theme} />
                </div>
                <span className="muted small">{s.label}</span>
              </li>
            ))}
          </ul>
        </section>
      </div>

      {dialog === "delete" && (
        <Modal
          title={t("songs.deleteTitle")}
          closeLabel={t("common.close")}
          onClose={() => setDialog(null)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setDialog(null)}>
                {t("common.cancel")}
              </Button>
              <Button variant="danger" onClick={() => void remove()}>
                {t("common.delete")}
              </Button>
            </>
          }
        >
          <p>{t("songs.deleteBody", { title: song.title })}</p>
        </Modal>
      )}
      {dialog === "split" && (
        <TextToSections
          onClose={() => setDialog(null)}
          onApply={(text) => {
            const r = suggestSections(text);
            if (r.sections.length === 0) return setDialog(null);
            const arrangements = song.arrangements.length
              ? song.arrangements.map((a, i) => (i === 0 ? { ...a, sequence: r.sequence } : a))
              : [{ id: `arr-${Date.now().toString(36)}`, name: "Original", sequence: r.sequence }];
            edit({ ...song, sections: r.sections, arrangements });
            setArrId(arrangements[0]!.id);
            setDialog(null);
          }}
        />
      )}
    </div>
  );
}
