import { Button, Field, SwitchRow, TextInput } from "@hollyshow/ui";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useHost } from "../../api/context";
import type { UsageRow } from "../../api/host";
import { useToasts } from "../../hooks/useToasts";

const DAY_MS = 86_400_000;

/** `yyyy-mm-dd` of a timestamp in local time (what `<input type="date">` uses). */
export function dateInput(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** Start of the local day `yyyy-mm-dd`. */
export function dayStart(value: string): number {
  const [y, m, d] = value.split("-").map(Number);
  return new Date(y ?? 1970, (m ?? 1) - 1, d ?? 1).getTime();
}

function csvCell(v: string | number): string {
  const s = String(v);
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** The report as CSV (title, CCLI number, times shown, last shown date). */
export function usageCsv(rows: UsageRow[], header: [string, string, string, string]): string {
  const lines = [
    header,
    ...rows.map((r) => [r.title ?? "", r.ccli ?? "", r.timesShown, dateInput(r.lastShownAt)]),
  ];
  return lines.map((l) => l.map(csvCell).join(",")).join("\n");
}

/** CCLI usage report and whole-library export / import. */
export function LibrarySettings() {
  const { t } = useTranslation();
  const host = useHost();
  const toasts = useToasts();
  const today = Date.now();
  const [from, setFrom] = useState(() => dateInput(today - 90 * DAY_MS));
  const [to, setTo] = useState(() => dateInput(today));
  const [rows, setRows] = useState<UsageRow[] | null>(null);
  const [includeBibles, setIncludeBibles] = useState(false);
  const [includeUsage, setIncludeUsage] = useState(true);
  const [replace, setReplace] = useState(false);
  const [busy, setBusy] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  const guard = async (fn: () => Promise<void>) => {
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      toasts.error(e);
    } finally {
      setBusy(false);
    }
  };

  const generate = () =>
    guard(async () => {
      // `to` is inclusive for the user, exclusive for the query.
      setRows(await host.usageReport(dayStart(from), dayStart(to) + DAY_MS));
    });

  const copyCsv = () =>
    guard(async () => {
      const header: [string, string, string, string] = [
        t("settings.library.colTitle"),
        t("settings.library.colCcli"),
        t("settings.library.colTimes"),
        t("settings.library.colLast"),
      ];
      await navigator.clipboard.writeText(usageCsv(rows ?? [], header));
      toasts.push(t("settings.library.copied"), "success");
    });

  const exportAll = () =>
    guard(async () => {
      const path = await host.exportLibrary({ includeBibles, includeUsage });
      toasts.push(t("settings.library.exported", { path }), "success");
    });

  const importFile = (file: File | undefined) => {
    if (!file) return;
    if (replace && !window.confirm(t("settings.library.confirmReplace"))) return;
    void guard(async () => {
      const text = await file.text();
      try {
        const r = await host.importLibrary(text, replace);
        toasts.push(
          t("settings.library.imported", {
            songs: r.songsAdded + r.songsUpdated,
            themes: r.themes,
            playlists: r.playlists,
          }),
          "success",
        );
      } catch (e) {
        toasts.error(e, t("settings.library.invalid"));
      }
    });
  };

  return (
    <div className="stack gap-16">
      <header>
        <h1>{t("settings.library.title")}</h1>
        <p className="muted">{t("settings.library.intro")}</p>
      </header>

      <section className="stack" aria-labelledby="usage-title">
        <h2 id="usage-title" className="subhead">
          {t("settings.library.reportTitle")}
        </h2>
        <p className="muted">{t("settings.library.reportHint")}</p>
        <div className="row gap-8" style={{ alignItems: "flex-end" }}>
          <Field label={t("settings.library.from")}>
            {(id) => (
              <TextInput
                id={id}
                type="date"
                value={from}
                onChange={(e) => setFrom(e.target.value)}
              />
            )}
          </Field>
          <Field label={t("settings.library.to")}>
            {(id) => (
              <TextInput id={id} type="date" value={to} onChange={(e) => setTo(e.target.value)} />
            )}
          </Field>
          <Button variant="primary" disabled={busy || !from || !to} onClick={() => void generate()}>
            {t("settings.library.generate")}
          </Button>
          <Button variant="ghost" disabled={busy || !rows?.length} onClick={() => void copyCsv()}>
            {t("settings.library.copyCsv")}
          </Button>
        </div>
        {rows &&
          (rows.length === 0 ? (
            <p className="muted">{t("settings.library.empty")}</p>
          ) : (
            <div className="table-wrap">
              <table className="verse-table" aria-label={t("settings.library.reportTable")}>
                <thead>
                  <tr>
                    <th scope="col">{t("settings.library.colTitle")}</th>
                    <th scope="col">{t("settings.library.colCcli")}</th>
                    <th scope="col">{t("settings.library.colTimes")}</th>
                    <th scope="col">{t("settings.library.colLast")}</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((r) => (
                    <tr key={r.songId}>
                      <td>{r.title ?? r.songId}</td>
                      <td>{r.ccli ?? "—"}</td>
                      <td>{r.timesShown}</td>
                      <td>{dateInput(r.lastShownAt)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}
      </section>

      <section className="stack" aria-labelledby="backup-title">
        <h2 id="backup-title" className="subhead">
          {t("settings.library.backupTitle")}
        </h2>
        <p className="muted">{t("settings.library.backupHint")}</p>
        <SwitchRow
          label={t("settings.library.includeBibles")}
          checked={includeBibles}
          onChange={setIncludeBibles}
        />
        <SwitchRow
          label={t("settings.library.includeUsage")}
          checked={includeUsage}
          onChange={setIncludeUsage}
        />
        <div>
          <Button disabled={busy} onClick={() => void exportAll()}>
            {t("settings.library.export")}
          </Button>
        </div>
        <SwitchRow label={t("settings.library.replace")} checked={replace} onChange={setReplace} />
        <div>
          <input
            ref={fileInput}
            type="file"
            accept="application/json,.json"
            hidden
            data-testid="library-file"
            aria-label={t("settings.library.import")}
            onChange={(e) => {
              importFile(e.target.files?.[0]);
              e.target.value = "";
            }}
          />
          <Button disabled={busy} onClick={() => fileInput.current?.click()}>
            {t("settings.library.import")}
          </Button>
        </div>
      </section>
    </div>
  );
}
