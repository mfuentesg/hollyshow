import type { PresentationState, ShowEntry, Slide } from "./generated";

export function lyricSlide(id: string, label: string | null, lines: string[]): Slide {
  return {
    id,
    kind: "lyrics",
    label,
    lines,
    translation: [],
    chart: [],
    bible: [],
    layout: "columns",
    media: null,
    footer: null,
    themeId: null,
    sizeGroup: null,
  };
}

/** Public-domain / invented sample text only. */
export function sampleShow(): ShowEntry[] {
  return [
    {
      itemId: "item-1",
      title: "Cancion de ejemplo",
      durationSecs: 240,
      slides: [
        lyricSlide("item-1:0", "Verso 1", ["Linea uno de ejemplo", "Linea dos de ejemplo"]),
        lyricSlide("item-1:1", "Coro", ["Coro de ejemplo", "Se repite aqui"]),
        lyricSlide("item-1:2", "Verso 2", ["Segundo verso de ejemplo", "Para probar la pantalla"]),
      ],
    },
    {
      itemId: "item-2",
      title: "Anuncios",
      durationSecs: 120,
      slides: [lyricSlide("item-2:0", null, ["Bienvenidos", "Reunion de jovenes el viernes"])],
    },
  ];
}

export function emptyState(): PresentationState {
  return {
    version: 0,
    showRevision: 0,
    show: [],
    live: null,
    preview: null,
    blackout: false,
    clear: false,
    logo: false,
    stageMessage: null,
    timers: [],
    outputs: {
      main: { kind: "main", themeId: null, enabled: true, followsBlackout: true },
      stage: { kind: "stage", themeId: null, enabled: false, followsBlackout: false },
    },
    outputLayers: {
      main: { background: null, media: null, text: true, overlay: null, blackout: false },
      stage: { background: null, media: null, text: true, overlay: null, blackout: false },
    },
    itemClock: null,
    serviceMode: false,
  };
}
