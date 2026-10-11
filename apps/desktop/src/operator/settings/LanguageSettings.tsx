import { Field, Select } from "@hollyshow/ui";
import { useTranslation } from "react-i18next";
import { useHost } from "../../api/context";

export const LANG_KEY = "ui.language";

/** UI language (i18n-ready: es default, en secondary). */
export function LanguageSettings() {
  const { t, i18n } = useTranslation();
  const host = useHost();
  const current = i18n.language.startsWith("en") ? "en" : "es";
  return (
    <div className="stack gap-16">
      <header>
        <h1>{t("settings.language.title")}</h1>
        <p className="muted">{t("settings.language.intro")}</p>
      </header>
      <Field label={t("settings.language.label")} style={{ maxWidth: 320 }}>
        {(id) => (
          <Select
            id={id}
            value={current}
            onChange={(e) => {
              void i18n.changeLanguage(e.target.value);
              void host.setSetting(LANG_KEY, e.target.value).catch(() => {});
            }}
          >
            <option value="es">Español</option>
            <option value="en">English</option>
          </Select>
        )}
      </Field>
    </div>
  );
}
