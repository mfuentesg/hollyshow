import "../../i18n";
import i18n from "i18next";
import en from "./update.en.json";
import es from "./update.es.json";

/** Namespace used by every component of this module (`useTranslation(UPDATE_NS)`). */
export const UPDATE_NS = "update";

let registered = false;

/** Adds the `update` namespace to the shared i18next instance (idempotent). */
export function registerUpdateI18n(): void {
  if (registered) return;
  registered = true;
  i18n.addResourceBundle("es", UPDATE_NS, es, true, true);
  i18n.addResourceBundle("en", UPDATE_NS, en, true, true);
}

registerUpdateI18n();
