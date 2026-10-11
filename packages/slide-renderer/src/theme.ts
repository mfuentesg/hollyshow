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
