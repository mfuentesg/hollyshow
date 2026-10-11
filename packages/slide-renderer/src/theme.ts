import type { Theme } from "@hollyshow/protocol";

/** Mirrors `Theme::default_theme()` in Rust (checked by a test against the generated type). */
export const defaultTheme: Theme = {
  id: "default",
  name: "Predeterminado",
  background: { kind: "color", color: "#0F1115" },
  font: {
    family: "Archivo",
    weight: 800,
    minPx: 36,
    maxPx: 120,
    uniformSize: true,
    italic: false,
    uppercase: false,
    lineHeight: 1.2,
  },
  textColor: "#FFFFFF",
  shadow: "0 2px 8px rgba(0,0,0,.6)",
  align: "center",
  margins: { top: 6, right: 8, bottom: 6, left: 8 },
  transition: { kind: "crossfade", durationMs: 250, lineByLine: false },
  translationScale: 0.6,
  showBibleVersionLabel: true,
  showCopyright: true,
  maxLinesPerSlide: 4,
};

/** Theme for an output: the slide's own theme wins, then the output's, then the default. */
export function pickTheme(themes: Theme[], ...ids: Array<string | null | undefined>): Theme {
  for (const id of ids) {
    if (!id) continue;
    const t = themes.find((x) => x.id === id);
    if (t) return t;
  }
  return themes.find((t) => t.id === defaultTheme.id) ?? defaultTheme;
}
