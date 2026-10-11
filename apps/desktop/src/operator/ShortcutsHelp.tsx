import { Kbd, Modal } from "@hollyshow/ui";
import { useTranslation } from "react-i18next";

const ROWS: [string, string][] = [
  ["← →  PgUp PgDn  Space", "shortcuts.slide"],
  ["Enter", "shortcuts.previewToLive"],
  ["V1 V2 V3  C  P  U  I  F", "shortcuts.jump"],
  ["Shift + V1 …", "shortcuts.jumpPreview"],
  ["B", "shortcuts.black"],
  ["X", "shortcuts.clear"],
  ["L", "shortcuts.logo"],
  ["Ctrl/⌘ + K", "shortcuts.palette"],
  ["Ctrl/⌘ + Z", "common.undo"],
  ["Ctrl/⌘ + Shift + Z", "common.redo"],
  ["Alt + ↑ ↓", "shortcuts.reorder"],
  ["?", "shortcuts.title"],
];

export function ShortcutsHelp({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  return (
    <Modal title={t("shortcuts.title")} closeLabel={t("common.close")} onClose={onClose}>
      <table className="shortcuts">
        <tbody>
          {ROWS.map(([keys, label]) => (
            <tr key={label + keys}>
              <th scope="row">
                <Kbd>{keys}</Kbd>
              </th>
              <td>{t(label)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Modal>
  );
}
