import type { Slide } from "@hollyshow/protocol";
import { Button, Field, Modal, TextArea } from "@hollyshow/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { cmd } from "../lib/cmd";
import { useOperator } from "./OperatorContext";

/**
 * Live edit (PLAN §2.5): fix a typo on the projected slide. Ephemeral: it overlays the show and is
 * dropped when another service is opened; the song in the library is not modified.
 */
export function LiveEditDialog({
  itemId,
  slideIndex,
  slide,
  onClose,
}: {
  itemId: string;
  slideIndex: number;
  slide: Slide;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const { send } = useOperator();
  const isBible = slide.bible.length > 0;
  const [text, setText] = useState(slide.lines.join("\n"));
  const [cols, setCols] = useState(slide.bible.map((c) => c.text));

  const apply = async () => {
    const next: Slide = isBible
      ? { ...slide, bible: slide.bible.map((c, i) => ({ ...c, text: cols[i] ?? c.text })) }
      : { ...slide, lines: text.split("\n"), chart: [] };
    const ack = await send(cmd.updateSlide(itemId, slideIndex, next));
    if (ack?.ok) onClose();
  };

  return (
    <Modal
      title={t("slides.liveEdit")}
      closeLabel={t("common.close")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" onClick={() => void apply()}>
            {t("slides.apply")}
          </Button>
        </>
      }
    >
      <p className="muted small">{t("slides.liveEditNote")}</p>
      {isBible ? (
        slide.bible.map((c, i) => (
          <Field key={c.version} label={`${c.reference} · ${c.version}`}>
            {(id) => (
              <TextArea
                id={id}
                rows={4}
                value={cols[i] ?? ""}
                data-autofocus={i === 0 ? true : undefined}
                onChange={(e) => setCols(cols.map((x, j) => (j === i ? e.target.value : x)))}
              />
            )}
          </Field>
        ))
      ) : (
        <Field label={t("slides.text")}>
          {(id) => (
            <TextArea
              id={id}
              rows={6}
              value={text}
              data-autofocus
              onChange={(e) => setText(e.target.value)}
            />
          )}
        </Field>
      )}
    </Modal>
  );
}
