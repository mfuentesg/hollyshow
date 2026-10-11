import { Button, Field, Modal, TextInput } from "@hollyshow/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useToasts } from "../hooks/useToasts";
import { useOperator } from "./OperatorContext";

/** Open a saved service, create one from a template, or save the current one as a template. */
export function TemplateDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const op = useOperator();
  const toasts = useToasts();
  const templates = op.playlists.filter((p) => p.isTemplate);
  const services = op.playlists.filter((p) => !p.isTemplate);
  const today = new Date().toISOString().slice(0, 10);
  const [name, setName] = useState("");
  const [date, setDate] = useState(today);
  const [tplName, setTplName] = useState(op.playlist ? op.playlist.name : "");

  const run = async (fn: () => Promise<void>) => {
    try {
      await fn();
      onClose();
    } catch (e) {
      toasts.error(e);
    }
  };

  return (
    <Modal
      title={t("service.templates")}
      closeLabel={t("common.close")}
      onClose={onClose}
      width={640}
    >
      <section className="stack" aria-labelledby="tpl-new">
        <h3 id="tpl-new" className="subhead">
          {t("service.newFromTemplate")}
        </h3>
        <div className="row gap-12 wrap">
          <Field label={t("service.serviceName")} style={{ flex: "2 1 200px" }}>
            {(id) => (
              <TextInput
                id={id}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t("service.serviceNamePlaceholder")}
              />
            )}
          </Field>
          <Field label={t("service.date")} style={{ flex: "1 1 140px" }}>
            {(id) => (
              <TextInput
                id={id}
                type="date"
                mono
                value={date}
                onChange={(e) => setDate(e.target.value)}
              />
            )}
          </Field>
        </div>
        {templates.length === 0 && <p className="muted">{t("service.noTemplates")}</p>}
        <ul className="picklist">
          {templates.map((tp) => (
            <li key={tp.id}>
              <button
                type="button"
                className="picklist__row"
                onClick={() =>
                  void run(() =>
                    op.createFromTemplate(
                      tp.id,
                      name.trim() || `${tp.name} · ${date}`,
                      date || null,
                    ),
                  )
                }
              >
                <span>{tp.name}</span>
                <span className="muted">{t("service.items", { count: tp.itemCount })}</span>
              </button>
            </li>
          ))}
        </ul>
      </section>

      <section className="stack" aria-labelledby="tpl-open">
        <h3 id="tpl-open" className="subhead">
          {t("service.openSaved")}
        </h3>
        <ul className="picklist">
          {services.map((s) => (
            <li key={s.id}>
              <button
                type="button"
                className="picklist__row"
                onClick={() => void run(() => op.openPlaylist(s.id))}
              >
                <span>{s.name}</span>
                <span className="muted mono">{s.date ?? ""}</span>
              </button>
            </li>
          ))}
          {services.length === 0 && <li className="muted pad">{t("service.noSaved")}</li>}
        </ul>
      </section>

      {op.playlist && (
        <section className="stack" aria-labelledby="tpl-save">
          <h3 id="tpl-save" className="subhead">
            {t("service.saveAsTemplate")}
          </h3>
          <div className="row gap-8">
            <Field label={t("service.templateName")} style={{ flex: 1 }}>
              {(id) => (
                <TextInput id={id} value={tplName} onChange={(e) => setTplName(e.target.value)} />
              )}
            </Field>
            <Button
              style={{ alignSelf: "flex-end" }}
              disabled={!tplName.trim()}
              onClick={() => void run(() => op.saveAsTemplate(tplName.trim()))}
            >
              {t("common.save")}
            </Button>
          </div>
        </section>
      )}
    </Modal>
  );
}
