import {
  Button,
  IconButton,
  IconCopy,
  IconDown,
  IconEdit,
  IconGrip,
  IconPlus,
  IconRedo,
  IconTrash,
  IconUndo,
  IconUp,
} from "@hollyshow/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNow } from "../hooks/useNow";
import { duplicateItem, moveItem, removeItem } from "../lib/playlistOps";
import { formatClock, formatDuration, schedule, totalDuration } from "../lib/schedule";
import { cmd } from "../lib/cmd";
import { AddItemDialog } from "./AddItemDialog";
import { describeItem } from "./describe";
import { ItemEditDialog } from "./ItemEditDialog";
import { useOperator } from "./OperatorContext";
import { TemplateDialog } from "./TemplateDialog";

/** «Orden del culto»: times, drag and drop reorder, templates, per-item actions. */
export function ServiceOrderPanel() {
  const { t } = useTranslation();
  const op = useOperator();
  const { playlist, p, songs, editPlaylist, selectedItemId, selectItem, send } = op;
  const [dialog, setDialog] = useState<null | "add" | "template" | { edit: string }>(null);
  const [drag, setDrag] = useState<{ from: number; over: number | null } | null>(null);
  const now = useNow(1000);

  const items = playlist?.items ?? [];
  const rows = schedule(items, op.startSecs);
  const liveId = p.view.live?.itemId ?? null;
  const clock = p.view.itemClock;
  const overtimeMs = clock ? Math.max(0, now - clock.startedAtMs - clock.plannedSecs * 1000) : 0;
  const elapsedSecs = clock ? Math.max(0, (now - clock.startedAtMs) / 1000) : 0;
  const total = totalDuration(items);
  const locked = p.view.serviceMode;

  const editing =
    typeof dialog === "object" && dialog !== null
      ? items.find((i) => i.id === dialog.edit)
      : undefined;

  return (
    <aside className="panel panel--left" aria-label={t("service.title")}>
      <div className="panel__head">
        <h2>{t("service.title")}</h2>
        <div className="row gap-4">
          <IconButton
            aria-label={t("common.undo")}
            title={`${t("common.undo")} (Ctrl+Z)`}
            disabled={!op.canUndo}
            onClick={op.undo}
          >
            <IconUndo />
          </IconButton>
          <IconButton
            aria-label={t("common.redo")}
            title={`${t("common.redo")} (Ctrl+Shift+Z)`}
            disabled={!op.canRedo}
            onClick={op.redo}
          >
            <IconRedo />
          </IconButton>
          <IconButton
            aria-label={t("service.addItem")}
            onClick={() => setDialog("add")}
            disabled={!playlist}
          >
            <IconPlus size={18} />
          </IconButton>
        </div>
      </div>

      <div className="row gap-8 wrap">
        <label className="inline-field">
          <span>{t("service.startsAt")}</span>
          <input
            type="time"
            className="hs-input hs-input--bg hs-input--mono"
            value={formatClock(op.startSecs)}
            onChange={(e) => e.target.value && op.setStartTime(e.target.value)}
            aria-label={t("service.startsAt")}
          />
        </label>
        <Button size="sm" onClick={() => setDialog("template")}>
          {t("service.templates")}
        </Button>
      </div>

      {!playlist && (
        <div className="empty">
          <p>{t("service.noService")}</p>
          <Button variant="primary" onClick={() => setDialog("template")}>
            {t("service.openOrCreate")}
          </Button>
        </div>
      )}

      <ul className="order" aria-label={t("service.title")}>
        {items.map((it, i) => {
          const live = it.id === liveId;
          const sel = it.id === selectedItemId;
          const row = rows[i]!;
          const isHeader = it.content.kind === "header";
          return (
            <li
              key={it.id}
              className={[
                "order-item",
                live && "is-live",
                sel && "is-selected",
                isHeader && "is-header",
                drag?.over === i &&
                  drag.from !== i &&
                  (drag.from < i ? "drop-after" : "drop-before"),
              ]
                .filter(Boolean)
                .join(" ")}
              draggable
              onDragStart={(e) => {
                e.dataTransfer.effectAllowed = "move";
                e.dataTransfer.setData("text/plain", it.id);
                setDrag({ from: i, over: null });
              }}
              onDragOver={(e) => {
                if (!drag) return;
                e.preventDefault();
                if (drag.over !== i) setDrag({ ...drag, over: i });
              }}
              onDrop={(e) => {
                e.preventDefault();
                if (drag) editPlaylist((pl) => moveItem(pl, drag.from, i));
                setDrag(null);
              }}
              onDragEnd={() => setDrag(null)}
              onKeyDown={(e) => {
                if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
                  e.preventDefault();
                  e.stopPropagation();
                  editPlaylist((pl) => moveItem(pl, i, i + (e.key === "ArrowUp" ? -1 : 1)));
                } else if (
                  e.key === "Delete" &&
                  e.target === e.currentTarget.querySelector(".order-item__main")
                ) {
                  editPlaylist((pl) => removeItem(pl, it.id));
                }
              }}
            >
              <span className="order-item__grip" aria-hidden="true">
                <IconGrip size={14} />
              </span>
              <button
                type="button"
                className="order-item__main"
                aria-current={sel ? "true" : undefined}
                onClick={() => selectItem(it.id)}
                onDoubleClick={() => {
                  selectItem(it.id);
                  void send(cmd.showItem(it.id, "live"));
                }}
              >
                <span className="order-item__time mono">
                  {isHeader ? "" : formatClock(row.startSecs)}
                </span>
                <span className="order-item__text">
                  <span className="order-item__title">{it.title}</span>
                  <span className="order-item__sub">
                    {live ? (
                      <>
                        <span className="live-tag">● {t("operator.live")}</span>
                        {clock && clock.itemId === it.id && (
                          <span className={overtimeMs > 0 ? "overtime" : undefined}>
                            {" · "}
                            {overtimeMs > 0
                              ? `+${formatDuration(overtimeMs / 1000)}`
                              : formatDuration(elapsedSecs)}{" "}
                            / {formatDuration(clock.plannedSecs)}
                          </span>
                        )}
                        {" · "}
                      </>
                    ) : null}
                    {describeItem(it, songs, t)}
                  </span>
                </span>
              </button>
              <span className="order-item__actions">
                <IconButton
                  aria-label={t("service.moveUp", { title: it.title })}
                  disabled={i === 0}
                  onClick={() => editPlaylist((pl) => moveItem(pl, i, i - 1))}
                >
                  <IconUp size={14} />
                </IconButton>
                <IconButton
                  aria-label={t("service.moveDown", { title: it.title })}
                  disabled={i === items.length - 1}
                  onClick={() => editPlaylist((pl) => moveItem(pl, i, i + 1))}
                >
                  <IconDown size={14} />
                </IconButton>
                <IconButton
                  aria-label={t("service.edit", { title: it.title })}
                  onClick={() => setDialog({ edit: it.id })}
                >
                  <IconEdit size={14} />
                </IconButton>
                <IconButton
                  aria-label={t("service.duplicate", { title: it.title })}
                  onClick={() => editPlaylist((pl) => duplicateItem(pl, it.id))}
                >
                  <IconCopy size={14} />
                </IconButton>
                <IconButton
                  aria-label={t("service.remove", { title: it.title })}
                  disabled={live && locked}
                  onClick={() => editPlaylist((pl) => removeItem(pl, it.id))}
                >
                  <IconTrash size={14} />
                </IconButton>
              </span>
            </li>
          );
        })}
      </ul>

      {playlist && (
        <div className="order-total">
          <span>{t("service.total", { time: formatDuration(total) })}</span>
          <span className="mono">
            {t("service.ends", { time: formatClock(op.startSecs + total) })}
          </span>
        </div>
      )}

      {dialog === "add" && <AddItemDialog onClose={() => setDialog(null)} />}
      {dialog === "template" && <TemplateDialog onClose={() => setDialog(null)} />}
      {editing && <ItemEditDialog item={editing} onClose={() => setDialog(null)} />}
    </aside>
  );
}
