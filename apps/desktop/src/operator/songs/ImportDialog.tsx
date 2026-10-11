import type { Song } from "@hollyshow/protocol";
import { Button, Field, Modal, Select, TextArea } from "@hollyshow/ui";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useHost } from "../../api/context";
import type { SongImportFile, SongImportFormat } from "../../api/host";
import { useToasts } from "../../hooks/useToasts";

const FORMATS: SongImportFormat[] = ["auto", "openlyrics", "chordpro", "opensong", "text"];

/** Import songs from files (OpenLyrics, ChordPro, OpenSong, plain text) or pasted text. */
export function ImportDialog({
  onClose,
  onImported,
}: {
  onClose: () => void;
  onImported: (songs: Song[]) => void;
}) {
  const { t } = useTranslation();
  const host = useHost();
  const toasts = useToasts();
  const [format, setFormat] = useState<SongImportFormat>("auto");
  const [files, setFiles] = useState<SongImportFile[]>([]);
  const [pasted, setPasted] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<[string, string][]>([]);
  const input = useRef<HTMLInputElement>(null);

  const pick = async (list: FileList | null) => {
    if (!list) return;
    const read = await Promise.all(
      Array.from(list).map(async (f) => ({ name: f.name, content: await f.text() })),
    );
    setFiles((cur) => [...cur, ...read]);
  };

  const all = (): SongImportFile[] =>
    [...files, ...(pasted.trim() ? [{ name: t("songs.pasted"), content: pasted }] : [])].map(
      (f) => ({ ...f, format }),
    );

  const run = async () => {
    setBusy(true);
    try {
      const r = await host.importSongs(all());
      setFailed(r.failed);
      if (r.imported.length) {
        toasts.push(t("songs.imported", { count: r.imported.length }), "success");
        onImported(r.imported);
      }
      if (r.failed.length === 0) onClose();
    } catch (e) {
      toasts.error(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t("songs.import")}
      closeLabel={t("common.close")}
      onClose={onClose}
      width={620}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={busy || all().length === 0}
            onClick={() => void run()}
          >
            {t("songs.importAction")}
          </Button>
        </>
      }
    >
      <div className="stack">
        <Field label={t("songs.importFormat")}>
          {(id) => (
            <Select
              id={id}
              value={format}
              onChange={(e) => setFormat(e.target.value as SongImportFormat)}
            >
              {FORMATS.map((f) => (
                <option key={f} value={f}>
                  {t(`songs.formats.${f}`)}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <div className="row gap-8">
          <input
            ref={input}
            type="file"
            multiple
            hidden
            data-testid="import-files"
            accept=".xml,.txt,.cho,.chordpro,.crd,.chopro,.pro,.onsong,.opensong"
            aria-label={t("songs.chooseFiles")}
            onChange={(e) => void pick(e.target.files)}
          />
          <Button onClick={() => input.current?.click()}>{t("songs.chooseFiles")}</Button>
          <span className="muted small">{t("songs.filesChosen", { count: files.length })}</span>
        </div>
        {files.length > 0 && (
          <ul className="picklist">
            {files.map((f, i) => (
              <li key={`${f.name}-${i}`} className="picklist__row">
                <span>{f.name}</span>
                <button
                  type="button"
                  className="link"
                  onClick={() => setFiles((cur) => cur.filter((_, j) => j !== i))}
                >
                  {t("common.remove")}
                </button>
              </li>
            ))}
          </ul>
        )}
        <Field label={t("songs.pasteText")} hint={t("songs.pasteHint")}>
          {(id, d) => (
            <TextArea
              id={id}
              aria-describedby={d}
              rows={8}
              value={pasted}
              onChange={(e) => setPasted(e.target.value)}
            />
          )}
        </Field>
        {failed.length > 0 && (
          <ul className="errors" role="alert">
            {failed.map(([name, why]) => (
              <li key={name}>
                {name}: {why}
              </li>
            ))}
          </ul>
        )}
      </div>
    </Modal>
  );
}
