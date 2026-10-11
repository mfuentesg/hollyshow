import i18n from "i18next";

/** Strings of the stage window live in their own namespace so the app bundles stay untouched. */
export const STAGE_NS = "stage-window";

const es = {
  now: "ACTUAL",
  next: "SIGUIENTE",
  remaining: "Quedan {{time}}",
  over: "Pasado +{{time}}",
  elapsed: "Transcurrido {{time}}",
  blackout: "PANTALLA EN NEGRO",
  chords: "Acordes",
  empty: "Sin contenido en vivo",
  kind: { song: "Canción", bible: "Biblia", text: "Texto", media: "Medio", other: "Contenido" },
};
const en = {
  now: "NOW",
  next: "NEXT",
  remaining: "{{time}} left",
  over: "Over +{{time}}",
  elapsed: "Elapsed {{time}}",
  blackout: "SCREEN BLACKED OUT",
  chords: "Chords",
  empty: "Nothing live",
  kind: { song: "Song", bible: "Bible", text: "Text", media: "Media", other: "Content" },
};

export function registerStageStrings(): void {
  i18n.addResourceBundle("es", STAGE_NS, es, true, true);
  i18n.addResourceBundle("en", STAGE_NS, en, true, true);
}
