import type { BibleVersion } from "@hollyshow/protocol";
import {
  Button,
  Field,
  IconButton,
  IconDown,
  IconTrash,
  IconUp,
  Select,
  SwitchRow,
  TextInput,
} from "@hollyshow/ui";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useApi, useHost } from "../../api/context";
import type { BibleCatalogEntry, BibleImportFormat } from "../../api/host";
import { useToasts } from "../../hooks/useToasts";
import { useOperator } from "../OperatorContext";

const FORMATS: BibleImportFormat[] = [
  "auto",
  "hsb",
  "osis",
  "usfm",
  "zefania",
  "opensong",
  "tabular",
];

function toBase64(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf);
  let bin = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk)
    bin += String.fromCharCode(...bytes.subarray(i, i + chunk));
  return btoa(bin);
}

/** Bible versions manager: install, import, enable, default / secondary, presentation order. */
export function BibleSettings() {
  const { t } = useTranslation();
  const api = useApi();
  const host = useHost();
  const op = useOperator();
  const toasts = useToasts();
  const [all, setAll] = useState<BibleVersion[]>([]);
  const [catalog, setCatalog] = useState<BibleCatalogEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [format, setFormat] = useState<BibleImportFormat>("auto");
  const [meta, setMeta] = useState({ code: "", name: "", language: "es", pd: false, license: "" });
  const file = useRef<HTMLInputElement>(null);

  const load = useCallback(async () => {
    const [v, c] = await Promise.all([
      api.query({ kind: "listBibleVersions" }),
      host.listBibleCatalog(),
    ]);
    setAll([...v.items].sort((a, b) => a.sortOrder - b.sortOrder));
    setCatalog(c);
  }, [api, host]);

  useEffect(() => {
    load().catch(toasts.error);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [host]);

  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await fn();
      await load();
      await op.reloadLibrary();
    } catch (e) {
      toasts.error(e);
    } finally {
      setBusy(false);
    }
  };

  const move = (i: number, d: number) => {
    const codes = all.map((v) => v.code);
    const j = i + d;
    if (j < 0 || j >= codes.length) return;
    [codes[i], codes[j]] = [codes[j]!, codes[i]!];
    void run(() => host.reorderBibleVersions(codes));
  };

  const doImport = async (f: File | undefined) => {
    if (!f) return;
    await run(async () => {
      const v = await host.importBible({
        fileName: f.name,
        dataBase64: toBase64(await f.arrayBuffer()),
        format,
        ...(meta.code ? { code: meta.code.trim().toUpperCase() } : {}),
        ...(meta.name ? { name: meta.name.trim() } : {}),
        language: meta.language,
        isPublicDomain: meta.pd,
        ...(meta.license ? { license: meta.license } : {}),
      });
      toasts.push(t("settings.bible.imported", { code: v.code }), "success");
    });
  };

  const notInstalled = catalog.filter((c) => !c.installed);

  return (
    <div className="stack gap-16">
      <header>
        <h1>{t("settings.bible.title")}</h1>
        <p className="muted">{t("settings.bible.intro")}</p>
      </header>

      <section className="stack" aria-labelledby="bv-h">
        <h2 id="bv-h" className="subhead">
          {t("settings.bible.installed")}
        </h2>
        {all.length === 0 && <p className="muted">{t("settings.bible.none")}</p>}
        <ul className="picklist">
          {all.map((v, i) => (
            <li key={v.code} className="picklist__row bible-row">
              <div className="grow">
                <strong className="mono">{v.code}</strong> <span>{v.name}</span>
                <div className="muted small">
                  {v.language} ·{" "}
                  {v.isPublicDomain
                    ? t("settings.bible.publicDomain")
                    : v.license || t("settings.bible.licensed")}{" "}
                  · {v.copyrightText}
                </div>
              </div>
              <label className="inline-field">
                <input
                  type="checkbox"
                  checked={v.isEnabled}
                  disabled={busy}
                  onChange={(e) =>
                    void run(() => host.updateBibleVersion(v.code, { isEnabled: e.target.checked }))
                  }
                />
                <span>{t("settings.bible.enabled")}</span>
              </label>
              <label className="inline-field">
                <input
                  type="radio"
                  name="bible-default"
                  checked={v.isDefault}
                  disabled={busy || !v.isEnabled}
                  onChange={() =>
                    void run(() => host.updateBibleVersion(v.code, { isDefault: true }))
                  }
                />
                <span>{t("settings.bible.default")}</span>
              </label>
              <label className="inline-field">
                <input
                  type="radio"
                  name="bible-secondary"
                  checked={v.isSecondary}
                  disabled={busy || !v.isEnabled}
                  onChange={() =>
                    void run(() => host.updateBibleVersion(v.code, { isSecondary: true }))
                  }
                />
                <span>{t("settings.bible.secondary")}</span>
              </label>
              <IconButton
                aria-label={t("settings.bible.up", { code: v.code })}
                disabled={busy || i === 0}
                onClick={() => move(i, -1)}
              >
                <IconUp size={14} />
              </IconButton>
              <IconButton
                aria-label={t("settings.bible.down", { code: v.code })}
                disabled={busy || i === all.length - 1}
                onClick={() => move(i, 1)}
              >
                <IconDown size={14} />
              </IconButton>
              <IconButton
                aria-label={t("settings.bible.remove", { code: v.code })}
                disabled={busy}
                onClick={() => void run(() => host.removeBible(v.code))}
              >
                <IconTrash size={14} />
              </IconButton>
            </li>
          ))}
        </ul>
        <p className="muted small">{t("settings.bible.limitHint")}</p>
      </section>

      <section className="stack" aria-labelledby="bc-h">
        <h2 id="bc-h" className="subhead">
          {t("settings.bible.catalog")}
        </h2>
        {notInstalled.length === 0 && <p className="muted">{t("settings.bible.catalogEmpty")}</p>}
        <ul className="picklist">
          {notInstalled.map((c) => (
            <li key={c.code} className="picklist__row">
              <div className="grow">
                <strong className="mono">{c.code}</strong> {c.name}
                <div className="muted small">
                  {c.language} · {c.isPublicDomain ? t("settings.bible.publicDomain") : c.license} ·{" "}
                  {c.source}
                </div>
              </div>
              <Button
                size="sm"
                disabled={busy}
                onClick={() => void run(() => host.installBible(c.code))}
              >
                {t("settings.bible.install")}
              </Button>
            </li>
          ))}
        </ul>
      </section>

      <section className="card" aria-labelledby="bi-h">
        <h2 id="bi-h" className="card__title">
          {t("settings.bible.importTitle")}
        </h2>
        <p className="muted small">{t("settings.bible.importHint")}</p>
        <div className="song-fields">
          <Field label={t("settings.bible.format")}>
            {(id) => (
              <Select
                id={id}
                value={format}
                onChange={(e) => setFormat(e.target.value as BibleImportFormat)}
              >
                {FORMATS.map((f) => (
                  <option key={f} value={f}>
                    {f === "auto" ? t("songs.formats.auto") : f.toUpperCase()}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label={t("settings.bible.code")}>
            {(id) => (
              <TextInput
                id={id}
                mono
                maxLength={12}
                value={meta.code}
                onChange={(e) => setMeta({ ...meta, code: e.target.value })}
              />
            )}
          </Field>
          <Field label={t("settings.bible.name")} className="span-2">
            {(id) => (
              <TextInput
                id={id}
                value={meta.name}
                onChange={(e) => setMeta({ ...meta, name: e.target.value })}
              />
            )}
          </Field>
          <Field label={t("settings.bible.lang")}>
            {(id) => (
              <TextInput
                id={id}
                mono
                maxLength={8}
                value={meta.language}
                onChange={(e) => setMeta({ ...meta, language: e.target.value })}
              />
            )}
          </Field>
          <Field label={t("settings.bible.license")} className="span-2">
            {(id) => (
              <TextInput
                id={id}
                value={meta.license}
                onChange={(e) => setMeta({ ...meta, license: e.target.value })}
              />
            )}
          </Field>
        </div>
        <SwitchRow
          label={t("settings.bible.isPd")}
          checked={meta.pd}
          onChange={(v) => setMeta({ ...meta, pd: v })}
        />
        <p className="muted small">{t("settings.bible.copyrightWarning")}</p>
        <input
          ref={file}
          type="file"
          hidden
          data-testid="bible-file"
          aria-label={t("settings.bible.chooseFile")}
          onChange={(e) => void doImport(e.target.files?.[0])}
        />
        <Button disabled={busy} onClick={() => file.current?.click()}>
          {t("settings.bible.chooseFile")}
        </Button>
      </section>
    </div>
  );
}
