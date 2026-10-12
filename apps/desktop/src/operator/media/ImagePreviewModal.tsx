import type { MediaLibraryItem, MediaResult } from "@hollyshow/protocol";
import { Button, Modal } from "@hollyshow/ui";
import { useState } from "react";
import { ExternalLink } from "../ExternalLink";
import { LicenseBadge } from "./LicenseBadge";
import { formatBytes } from "./license";
import type { MediaT } from "./strings";

export interface ImagePreviewModalProps {
  result: MediaResult;
  /** Present once the image is in the local library. */
  downloaded: MediaLibraryItem | null;
  busy: boolean;
  onDownload: () => void;
  onUse: () => void;
  onClose: () => void;
  t: MediaT;
  /** Image source for the large preview (the thumbnail keeps it light and fast). */
  imageSrc: string;
}

/** Preview before downloading: full attribution, license and the Descargar / Usar como fondo actions. */
export function ImagePreviewModal({
  result,
  downloaded,
  busy,
  onDownload,
  onUse,
  onClose,
  t,
  imageSrc,
}: ImagePreviewModalProps) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(result.attribution);
      setCopied(true);
    } catch {
      /* clipboard may be unavailable; the text stays selectable */
    }
  };
  const size = result.width > 0 ? `${result.width} × ${result.height}` : "—";
  return (
    <Modal
      title={result.title}
      onClose={onClose}
      width={720}
      closeLabel={t("close")}
      footer={
        <div className="media-actions">
          <Button onClick={onClose}>{t("close")}</Button>
          {downloaded ? (
            <Button variant="primary" onClick={onUse} data-autofocus>
              {t("useAsBackground")}
            </Button>
          ) : (
            <>
              <Button disabled={busy} onClick={onDownload} data-autofocus>
                {busy ? t("downloading") : t("download")}
              </Button>
              <Button variant="primary" disabled={busy} onClick={onUse}>
                {t("useAsBackground")}
              </Button>
            </>
          )}
        </div>
      }
    >
      <div className="media-preview">
        <img className="media-preview__img" src={imageSrc} alt={result.title} />
        <dl>
          <dt>{t("author")}</dt>
          <dd>
            {result.creator ? (
              result.creatorUrl ? (
                <ExternalLink href={result.creatorUrl}>{result.creator}</ExternalLink>
              ) : (
                result.creator
              )
            ) : (
              t("unknownAuthor")
            )}
          </dd>
          <dt>{t("license")}</dt>
          <dd>
            <LicenseBadge license={result.license} />{" "}
            {result.licenseUrl && <ExternalLink href={result.licenseUrl} />}
          </dd>
          <dt>{t("origin")}</dt>
          <dd>
            <ExternalLink href={result.sourcePage}>
              {result.source === "openverse" ? t("sourceOpenverse") : t("sourceCommons")}
            </ExternalLink>
          </dd>
          <dt>{t("size")}</dt>
          <dd>{downloaded ? `${size} · ${formatBytes(downloaded.bytes)}` : size}</dd>
        </dl>
        <div>
          <h3 className="media-note">{t("attribution")}</h3>
          <p className="media-attribution">{result.attribution}</p>
          <Button size="sm" onClick={() => void copy()}>
            {copied ? t("copied") : t("copyAttribution")}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
