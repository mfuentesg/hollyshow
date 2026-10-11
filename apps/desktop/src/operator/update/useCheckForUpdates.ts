import { useTranslation } from "react-i18next";
import { UPDATE_NS } from "./i18n";
import { useUpdate } from "./UpdateContext";

/**
 * Entry for the command palette and the Help menu: `{ id, label, run }` with the label
 * "Buscar actualizaciones…". Disabled while a check or download is in flight.
 */
export function useCheckForUpdatesEntry() {
  const { t } = useTranslation(UPDATE_NS);
  const { checkNow, state } = useUpdate();
  return {
    id: "update-check",
    label: t("checkNow"),
    disabled: state.state === "checking" || state.state === "downloading",
    run: checkNow,
  };
}
