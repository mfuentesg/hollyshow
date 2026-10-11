import type { BibleVersion, Playlist, Song, Theme, Verse } from "@hollyshow/protocol";
import { defaultTheme } from "@hollyshow/slide-renderer";
import type { BibleCatalogEntry } from "../host";

/** Demo data for `pnpm dev:web` and tests. Public-domain or invented text only. */

const sec = (
  id: string,
  kind: Song["sections"][number]["kind"],
  label: string,
  lyrics: string,
) => ({
  id,
  kind,
  label,
  lyrics,
  translation: null,
});

export function sampleSongs(): Song[] {
  const now = Date.UTC(2026, 9, 1);
  const base = {
    altTitle: null,
    ccli: null,
    bpm: null,
    tags: [] as string[],
    themeId: null,
    createdAt: now,
    updatedAt: now,
  };
  return [
    {
      ...base,
      id: "song-santo",
      title: "Santo, santo, santo",
      author: "R. Heber / trad. J. B. Cabrera",
      copyright: "Dominio público",
      key: "D",
      tags: ["adoración", "himno"],
      sections: [
        sec(
          "v1",
          "verse",
          "Verso 1",
          "[D]¡Santo! ¡Santo! ¡Santo! Señor omnipotente,\nsiempre el labio mío loores te dará;\n---\n¡Santo! ¡Santo! ¡Santo! te adoro reverente,\nDios en tres personas, bendita Trinidad.",
        ),
        sec(
          "v2",
          "verse",
          "Verso 2",
          "¡Santo! ¡Santo! ¡Santo! en numeroso coro,\nsantos escogidos te adoran sin cesar,\n---\nde alegría llenos, y sus coronas de oro\nrinden ante el trono y el cristalino mar.",
        ),
        sec(
          "v3",
          "verse",
          "Verso 3",
          "¡Santo! ¡Santo! ¡Santo! la inmensa muchedumbre\nde ángeles que cumplen tu santa voluntad,\n---\nante ti se postra bañada de tu lumbre,\nante ti que has sido, que eres y serás.",
        ),
        sec("f", "outro", "Final", "Dios en tres personas,\nbendita Trinidad."),
      ],
      arrangements: [
        { id: "a1", name: "Domingo", sequence: ["v1", "v2", "v3", "v1", "f"] },
        { id: "a2", name: "Corto", sequence: ["v1", "f"] },
      ],
    },
    {
      ...base,
      id: "song-grace",
      title: "Amazing Grace",
      author: "John Newton",
      copyright: "Public domain",
      key: "G",
      tags: ["gracia"],
      sections: [
        sec(
          "v1",
          "verse",
          "Verso 1",
          "[G]Amazing grace! how [C]sweet the [G]sound,\nThat saved a wretch like me!\n---\nI [G]once was lost, but [D]now am found,\nWas [G]blind, but now I see.",
        ),
        sec(
          "v2",
          "verse",
          "Verso 2",
          "'Twas grace that taught my heart to fear,\nAnd grace my fears relieved;\n---\nHow precious did that grace appear\nThe hour I first believed.",
        ),
      ],
      arrangements: [{ id: "a1", name: "Original", sequence: ["v1", "v2"] }],
    },
    {
      ...base,
      id: "song-ejemplo",
      title: "Canción de ejemplo",
      author: null,
      copyright: null,
      key: "E",
      tags: ["ejemplo"],
      sections: [
        sec("v1", "verse", "Verso 1", "Línea uno de ejemplo\nLínea dos de ejemplo"),
        sec("c", "chorus", "Coro", "Coro de ejemplo\nSe repite aquí"),
      ],
      arrangements: [{ id: "a1", name: "Original", sequence: ["v1", "c", "v1", "c"] }],
    },
  ];
}

export function sampleThemes(): Theme[] {
  return [
    defaultTheme,
    {
      ...defaultTheme,
      id: "noche-azul",
      name: "Noche azul",
      background: { kind: "gradient", from: "#0B1A2E", to: "#10243F", angleDeg: 160 },
    },
    {
      ...defaultTheme,
      id: "claro",
      name: "Claro",
      background: { kind: "color", color: "#F6F4EE" },
      textColor: "#14161B",
      shadow: null,
    },
  ];
}

export function samplePlaylists(): Playlist[] {
  const item = (
    id: string,
    title: string,
    content: Playlist["items"][number]["content"],
    durationSecs: number | null,
    notes: string | null = null,
  ) => ({
    id,
    title,
    content,
    durationSecs,
    themeId: null,
    notes,
  });
  const john = {
    start: { book: "John", chapter: 3, verse: 16 },
    end: { book: "John", chapter: 3, verse: 18 },
  };
  const items: Playlist["items"] = [
    item("i-count", "Cuenta regresiva", { kind: "countdown", seconds: 300 }, 300),
    item("i-welcome", "Bienvenida", { kind: "text", body: "Bienvenidos\nDios les bendiga" }, 180),
    item(
      "i-santo",
      "Santo, santo, santo",
      { kind: "song", songId: "song-santo", arrangementId: "a1", transpose: 0 },
      360,
    ),
    item(
      "i-grace",
      "Amazing Grace",
      { kind: "song", songId: "song-grace", arrangementId: null, transpose: 0 },
      360,
    ),
    item(
      "i-john",
      "Lectura: Juan 3:16-18",
      { kind: "bible", passage: john, versions: ["RVR1909", "KJV"], layout: "columns" },
      300,
    ),
    item(
      "i-sermon",
      "Prédica",
      { kind: "text", body: "Prédica" },
      2100,
      "Tres puntos: gracia, fe, vida.",
    ),
    item("i-ann", "Anuncios", { kind: "header" }, null),
  ];
  return [
    {
      id: "svc-main",
      name: "Domingo 12 oct · Culto principal",
      date: "2026-10-11",
      isTemplate: false,
      items,
    },
    {
      id: "tpl-sunday",
      name: "Domingo AM",
      date: null,
      isTemplate: true,
      items: items.slice(0, 5).map((i) => ({ ...i, id: `t-${i.id}` })),
    },
  ];
}

const v = (book: string, chapter: number, verse: number, text: string): Verse => ({
  reference: { book, chapter, verse },
  text,
  marks: [],
});

export function sampleBibleVersions(): BibleVersion[] {
  const base = { isPublicDomain: true, versification: "standard", isEnabled: true };
  return [
    {
      ...base,
      code: "RVR1909",
      name: "Reina-Valera 1909",
      language: "es",
      license: "Dominio público",
      copyrightText: "Reina-Valera 1909 (dominio público)",
      sortOrder: 0,
      isDefault: true,
      isSecondary: false,
    },
    {
      ...base,
      code: "KJV",
      name: "King James Version",
      language: "en",
      license: "Public domain",
      copyrightText: "King James Version (public domain)",
      sortOrder: 1,
      isDefault: false,
      isSecondary: true,
    },
  ];
}

export function sampleBibleCatalog(): BibleCatalogEntry[] {
  return [
    {
      code: "RVR1909",
      name: "Reina-Valera 1909",
      language: "es",
      license: "Dominio público",
      isPublicDomain: true,
      source: "Texto de dominio público incluido con la aplicación",
      installed: true,
    },
    {
      code: "KJV",
      name: "King James Version",
      language: "en",
      license: "Public domain",
      isPublicDomain: true,
      source: "Bundled public-domain text",
      installed: true,
    },
    {
      code: "RV1865",
      name: "Reina-Valera Antigua 1865",
      language: "es",
      license: "Dominio público",
      isPublicDomain: true,
      source: "Texto de dominio público incluido con la aplicación",
      installed: false,
    },
  ];
}

/** Verses of the demo Bibles (a few chapters), keyed by version code. */
export function sampleVerses(): Record<string, Verse[]> {
  const jnEs = [
    "Y como Moisés levantó la serpiente en el desierto, así es necesario que el Hijo del hombre sea levantado,",
    "Para que todo aquel que en él creyere, no se pierda, sino que tenga vida eterna.",
    "Porque de tal manera amó Dios al mundo, que ha dado á su Hijo unigénito, para que todo aquel que en él cree, no se pierda, mas tenga vida eterna.",
    "Porque no envió Dios á su Hijo al mundo, para que condene al mundo, mas para que el mundo sea salvo por él.",
    "El que en él cree, no es condenado; mas el que no cree, ya es condenado, porque no creyó en el nombre del unigénito Hijo de Dios.",
  ];
  const jnEn = [
    "And as Moses lifted up the serpent in the wilderness, even so must the Son of man be lifted up:",
    "That whosoever believeth in him should not perish, but have eternal life.",
    "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.",
    "For God sent not his Son into the world to condemn the world; but that the world through him might be saved.",
    "He that believeth on him is not condemned: but he that believeth not is condemned already, because he hath not believed in the name of the only begotten Son of God.",
  ];
  const psEs = [
    "Jehová es mi pastor; nada me faltará.",
    "En lugares de delicados pastos me hará yacer; Junto á aguas de reposo me pastoreará.",
    "Confortará mi alma; Guiárame por sendas de justicia por amor de su nombre.",
    "Aunque ande en valle de sombra de muerte, no temeré mal alguno; porque tú estarás conmigo: tu vara y tu cayado me infundirán aliento.",
    "Aderezarás mesa delante de mí en presencia de mis angustiadores: ungiste mi cabeza con aceite; mi copa está rebosando.",
    "Ciertamente el bien y la misericordia me seguirán todos los días de mi vida: y en la casa de Jehová moraré por largos días.",
  ];
  const psEn = [
    "The LORD is my shepherd; I shall not want.",
    "He maketh me to lie down in green pastures: he leadeth me beside the still waters.",
    "He restoreth my soul: he leadeth me in the paths of righteousness for his name's sake.",
    "Yea, though I walk through the valley of the shadow of death, I will fear no evil: for thou art with me; thy rod and thy staff they comfort me.",
    "Thou preparest a table before me in the presence of mine enemies: thou anointest my head with oil; my cup runneth over.",
    "Surely goodness and mercy shall follow me all the days of my life: and I will dwell in the house of the LORD for ever.",
  ];
  const build = (jn: string[], ps: string[]) => [
    ...ps.map((t, i) => v("Ps", 23, i + 1, t)),
    ...jn.map((t, i) => v("John", 3, i + 14, t)),
  ];
  return { RVR1909: build(jnEs, psEs), KJV: build(jnEn, psEn) };
}
