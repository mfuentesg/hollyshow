import type {
  MediaApi,
  MediaCategory,
  MediaLibraryItem,
  MediaResult,
  MediaSearchRequest,
  MediaSearchResponse,
} from "./media";

const CATEGORIES: MediaCategory[] = [
  ["cross", "Cruz", "Cross", "cruz", "cross"],
  ["sky", "Cielo y nubes", "Sky and clouds", "cielo nubes", "sky clouds"],
  ["sunrise", "Amanecer", "Sunrise", "amanecer", "sunrise"],
  ["nature", "Naturaleza", "Nature", "naturaleza paisaje", "nature landscape"],
  ["church", "Iglesia", "Church", "iglesia", "church interior"],
  ["stained-glass", "Vitrales", "Stained glass", "vitrales", "stained glass"],
  ["bible", "Biblia", "Bible", "biblia", "bible"],
  ["christmas", "Navidad", "Christmas", "navidad", "christmas nativity"],
  ["easter", "Semana Santa", "Holy Week", "semana santa", "easter"],
  ["baptism", "Bautismo", "Baptism", "bautismo", "baptism"],
  ["communion", "Santa Cena", "Communion", "santa cena", "communion bread wine"],
].map(
  ([id, labelEs, labelEn, queryEs, queryEn]) =>
    ({ id, labelEs, labelEn, queryEs, queryEn }) as MediaCategory,
);

function svgData(a: string, b: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="360"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient></defs><rect width="640" height="360" fill="url(#g)"/></svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

type Sample = MediaResult & { tags: string[] };

const SAMPLE: Sample[] = [
  mk(
    "m1",
    "openverse",
    "Cruz de madera al amanecer",
    "#F2B544",
    "#1F232C",
    "Jane Doe",
    "CC BY 4.0",
    ["cross", "sunrise"],
  ),
  mk("m2", "openverse", "Cielo con nubes doradas", "#2563D9", "#E8EAF0", null, "CC0 1.0", [
    "sky",
    "clouds",
  ]),
  mk("m3", "commons", "Vitral de catedral", "#D13438", "#2563D9", "María Pérez", "CC BY-SA 4.0", [
    "stained",
    "glass",
  ]),
  mk("m4", "commons", "Pantocrátor (icono antiguo)", "#2FA36B", "#171A21", null, "PDM", [
    "cross",
    "icon",
  ]),
  mk("m5", "openverse", "Lago en la montaña", "#5B9BFF", "#0F1115", "Pat Lee", "CC BY 2.0", [
    "nature",
    "lake",
  ]),
];

function mk(
  id: string,
  source: MediaResult["source"],
  title: string,
  a: string,
  b: string,
  creator: string | null,
  license: string,
  tags: string[],
): Sample {
  const data = svgData(a, b);
  const r: Sample = {
    source,
    id,
    title,
    thumbUrl: data,
    fullUrl: data,
    width: 3840,
    height: 2160,
    creator,
    creatorUrl: creator ? "https://example.org/autor" : null,
    license,
    licenseUrl: "https://creativecommons.org/licenses/by/4.0/",
    attribution: creator
      ? `"${title}" by ${creator} is licensed under ${license}.`
      : `"${title}" is marked with ${license}.`,
    sourcePage: "https://example.org/imagen",
    tags,
  };
  return r;
}

function norm(s: string): string {
  return s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** In-memory image bank for `createMockApi` and the demo adapter. */
export function createMockMedia(): MediaApi {
  const library = new Map<string, MediaLibraryItem>();
  return {
    async mediaSearch(req: MediaSearchRequest): Promise<MediaSearchResponse> {
      const words = norm(req.query).split(/\s+/).filter(Boolean);
      const sourceOk = (r: MediaResult) =>
        !req.source || req.source === "all" || r.source === req.source;
      const hit = (r: Sample) =>
        words.length === 0 ||
        words.some((w) => norm(r.title).includes(w) || r.tags.some((t) => t.includes(w)));
      const results = SAMPLE.filter((r) => sourceOk(r) && hit(r)).map((r) => {
        const { tags: _t, ...rest } = r;
        return rest;
      });
      return {
        results: (req.page ?? 1) > 1 ? [] : results,
        page: req.page ?? 1,
        hasMore: false,
        translatedQuery: req.query,
        failures: [],
      };
    },
    async mediaCategories() {
      return CATEGORIES;
    },
    async mediaDownload(result) {
      const id = `${result.source}-${result.id}`;
      const existing = library.get(id);
      if (existing) return existing;
      const item: MediaLibraryItem = {
        id,
        path: `/media/${id}.jpg`,
        thumbPath: result.thumbUrl,
        width: result.width,
        height: result.height,
        bytes: 1_200_000,
        downloadedAtMs: Date.now(),
        result,
      };
      library.set(id, item);
      return item;
    },
    async mediaList() {
      return [...library.values()].sort((a, b) => b.downloadedAtMs - a.downloadedAtMs);
    },
    async mediaRemove(id) {
      return library.delete(id);
    },
    async mediaCredits(ids) {
      const items = [...library.values()].filter((i) => !ids || ids.includes(i.id));
      if (items.length === 0)
        return "# Créditos de imágenes\n\nNo hay imágenes descargadas todavía.\n";
      return (
        "# Créditos de imágenes\n\n" +
        items
          .map(
            (i) =>
              `- **${i.result.title}** — ${i.result.creator ?? "Autor desconocido"} · ${i.result.license}\n  - ${i.result.attribution}\n`,
          )
          .join("")
      );
    },
  };
}
