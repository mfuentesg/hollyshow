import type { ServiceItem, SongSummary } from "@hollyshow/protocol";
import type { TFunction } from "i18next";
import { transposeKey } from "../lib/chords";
import { formatDuration } from "../lib/schedule";

/** Second line of a service row: "Canción · G", "Biblia · RVR1909 + KJV", "Temporizador · 5 min". */
export function describeItem(item: ServiceItem, songs: SongSummary[], t: TFunction): string {
  const c = item.content;
  switch (c.kind) {
    case "song": {
      const s = songs.find((x) => x.id === c.songId);
      const key = transposeKey(s?.key ?? null, c.transpose);
      return `${t("service.kind.song")} · ${key ?? "—"}`;
    }
    case "bible":
      return `${t("service.kind.bible")} · ${c.versions.join(" + ") || "—"}`;
    case "media":
      return `${t("service.kind.media")}${item.durationSecs ? ` · ${formatDuration(item.durationSecs)}` : ""}`;
    case "text":
      return `${t("service.kind.text")}${item.durationSecs ? ` · ${Math.round(item.durationSecs / 60)} min` : ""}`;
    case "countdown":
      return `${t("service.kind.countdown")} · ${Math.round(c.seconds / 60)} min`;
    case "header":
      return t("service.kind.header");
  }
}
