import type { Slide, TextAlign, Theme, TransitionKind } from "@hollyshow/protocol";
import { SlideView, defaultTheme } from "@hollyshow/slide-renderer";
import {
  Button,
  Field,
  IconRedo,
  IconButton,
  IconUndo,
  Modal,
  Select,
  SwitchRow,
  TextInput,
} from "@hollyshow/ui";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useHost } from "../../api/context";
import { useToasts } from "../../hooks/useToasts";
import { useHistory } from "../../lib/history";
import { clamp, uid } from "../../lib/text";
import { useOperator } from "../OperatorContext";
import { ImageSearchPanel } from "../media";

const FONTS = [
  "Archivo",
  "Instrument Sans",
  "JetBrains Mono",
  "Georgia",
  "Arial",
  "Verdana",
  "Times New Roman",
];
const TRANSITIONS: TransitionKind[] = [
  "cut",
  "fade",
  "crossfade",
  "slide",
  "zoom",
  "blur",
  "shader",
];
const ALIGNS: TextAlign[] = ["left", "center", "right"];

type Sample = "lyrics" | "bible";

function sampleSlide(kind: Sample, t: (k: string) => string): Slide {
  const base: Slide = {
    id: "theme-sample",
    kind: "lyrics",
    label: t("themes.sampleLabel"),
    lines: [t("themes.sampleLine1"), t("themes.sampleLine2")],
    translation: [],
    chart: [],
    bible: [],
    layout: "columns",
    media: null,
    footer: t("themes.sampleFooter"),
    themeId: null,
    sizeGroup: null,
  };
  if (kind === "lyrics") return base;
  return {
    ...base,
    kind: "bible",
    label: "Juan 3:16",
    lines: [],
    footer: "RVR1909",
    bible: [
      {
        version: "RVR1909",
        reference: "Juan 3:16",
        text: t("themes.sampleVerse"),
        copyright: "RVR1909",
      },
    ],
  };
}

function newTheme(name: string): Theme {
  return {
    ...defaultTheme,
    id: uid("theme"),
    name,
    background: { ...defaultTheme.background } as Theme["background"],
  };
}

/** Themes editor with a live preview of lyrics or a Bible slide. */
export function ThemesPage() {
  const { t } = useTranslation();
  const host = useHost();
  const op = useOperator();
  const toasts = useToasts();
  const h = useHistory<Theme>(op.themes[0] ?? defaultTheme);
  const [saved, setSaved] = useState<Theme>(h.value);
  const [sample, setSample] = useState<Sample>("lyrics");
  const [picking, setPicking] = useState(false);
  const theme = h.value;
  const dirty = JSON.stringify(theme) !== JSON.stringify(saved);

  const load = (th: Theme) => {
    h.reset(th);
    setSaved(th);
  };
  // First load once the library arrives.
  useEffect(() => {
    if (!op.themes.some((x) => x.id === h.value.id) && op.themes[0]) load(op.themes[0]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [op.themes]);

  const edit = (patch: Partial<Theme>, key?: string) => h.set({ ...theme, ...patch }, key);
  const num = (v: string, lo: number, hi: number, fallback: number) =>
    clamp(Number.isFinite(parseFloat(v)) ? parseFloat(v) : fallback, lo, hi);

  const save = async () => {
    try {
      const out = await host.saveTheme({
        ...theme,
        name: theme.name.trim() || t("themes.untitled"),
      });
      load(out);
      await op.reloadLibrary();
      toasts.push(t("themes.savedToast", { name: out.name }), "success");
    } catch (e) {
      toasts.error(e);
    }
  };

  const remove = async () => {
    try {
      await host.deleteTheme(theme.id);
      await op.reloadLibrary();
      load(op.themes.find((x) => x.id !== theme.id) ?? defaultTheme);
    } catch (e) {
      toasts.error(e);
    }
  };

  const bg = theme.background;
  const bgKind =
    bg.kind === "color" || bg.kind === "gradient" || bg.kind === "image" ? bg.kind : "other";
  const m = theme.margins;

  return (
    <div className="songs-page">
      <aside className="panel panel--left" aria-label={t("themes.library")}>
        <Button
          variant="primary"
          size="sm"
          onClick={() => {
            const th = newTheme(t("themes.newName"));
            h.reset(th);
            setSaved({ ...th, id: "" });
          }}
        >
          + {t("themes.new")}
        </Button>
        <ul className="song-list">
          {op.themes.map((th) => (
            <li key={th.id}>
              <button
                type="button"
                className={`song-row${th.id === theme.id ? " active" : ""}`}
                aria-current={th.id === theme.id ? "true" : undefined}
                onClick={() => load(th)}
              >
                <span className="song-row__title">{th.name}</span>
                <span className="muted small">
                  {th.font.family} · {t(`themes.transitions.${th.transition.kind}`)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </aside>

      <main className="center" aria-label={t("themes.editor")}>
        <div className="row gap-8 wrap song-editor__bar">
          <h1 className="grow">{t("themes.title")}</h1>
          <IconButton aria-label={t("common.undo")} disabled={!h.canUndo} onClick={() => h.undo()}>
            <IconUndo />
          </IconButton>
          <IconButton aria-label={t("common.redo")} disabled={!h.canRedo} onClick={() => h.redo()}>
            <IconRedo />
          </IconButton>
          <Button
            size="sm"
            variant="danger"
            disabled={
              theme.id === "" ||
              theme.id === defaultTheme.id ||
              !op.themes.some((x) => x.id === theme.id)
            }
            onClick={() => void remove()}
          >
            {t("common.delete")}
          </Button>
          <Button variant="primary" disabled={!dirty} onClick={() => void save()}>
            {t("common.save")}
          </Button>
        </div>

        <div className="song-split">
          <div className="stack">
            <div className="song-fields">
              <Field label={t("themes.name")} className="span-2">
                {(id) => (
                  <TextInput
                    id={id}
                    value={theme.name}
                    onChange={(e) => edit({ name: e.target.value }, "name")}
                  />
                )}
              </Field>
              <Field label={t("themes.align")}>
                {(id) => (
                  <Select
                    id={id}
                    value={theme.align}
                    onChange={(e) => edit({ align: e.target.value as TextAlign })}
                  >
                    {ALIGNS.map((a) => (
                      <option key={a} value={a}>
                        {t(`themes.aligns.${a}`)}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
              <Field label={t("themes.textColor")}>
                {(id) => (
                  <input
                    id={id}
                    type="color"
                    className="hs-input hs-input--color"
                    value={theme.textColor.slice(0, 7)}
                    onChange={(e) => edit({ textColor: e.target.value }, "textColor")}
                  />
                )}
              </Field>
            </div>

            <fieldset className="fieldset">
              <legend>{t("themes.background")}</legend>
              <div className="song-fields">
                <Field label={t("themes.backgroundKind")}>
                  {(id) => (
                    <Select
                      id={id}
                      value={bgKind}
                      onChange={(e) => {
                        if (e.target.value === "image") {
                          // The image comes from the bank / library: pick it before switching.
                          if (bg.kind !== "image") setPicking(true);
                          return;
                        }
                        edit({
                          background:
                            e.target.value === "gradient"
                              ? { kind: "gradient", from: "#0B1A2E", to: "#0F1115", angleDeg: 180 }
                              : {
                                  kind: "color",
                                  color: bg.kind === "gradient" ? bg.from : "#0F1115",
                                },
                        });
                      }}
                    >
                      <option value="color">{t("themes.bg.color")}</option>
                      <option value="gradient">{t("themes.bg.gradient")}</option>
                      <option value="image">{t("themes.bg.image")}</option>
                      {bgKind === "other" && (
                        <option value="other">{t(`themes.bg.${bg.kind}`)}</option>
                      )}
                    </Select>
                  )}
                </Field>
                {bg.kind === "color" && (
                  <Field label={t("themes.bg.color")}>
                    {(id) => (
                      <input
                        id={id}
                        type="color"
                        className="hs-input hs-input--color"
                        value={bg.color.slice(0, 7)}
                        onChange={(e) =>
                          edit({ background: { kind: "color", color: e.target.value } }, "bg")
                        }
                      />
                    )}
                  </Field>
                )}
                {bg.kind === "image" && (
                  <>
                    <Field label={t("themes.blur")}>
                      {(id) => (
                        <TextInput
                          id={id}
                          type="number"
                          mono
                          min={0}
                          max={40}
                          value={bg.blurPx}
                          onChange={(e) =>
                            edit(
                              { background: { ...bg, blurPx: num(e.target.value, 0, 40, 0) } },
                              "bg-blur",
                            )
                          }
                        />
                      )}
                    </Field>
                    <SwitchRow
                      label={t("themes.kenBurns")}
                      checked={bg.kenBurns}
                      onChange={(v) => edit({ background: { ...bg, kenBurns: v } })}
                    />
                    <div className="row gap-8">
                      <Button size="sm" onClick={() => setPicking(true)}>
                        {t("themes.pickImage")}
                      </Button>
                      <span className="muted small mono">{bg.mediaId}</span>
                    </div>
                  </>
                )}
                {bg.kind === "gradient" && (
                  <>
                    <Field label={t("themes.gradFrom")}>
                      {(id) => (
                        <input
                          id={id}
                          type="color"
                          className="hs-input hs-input--color"
                          value={bg.from.slice(0, 7)}
                          onChange={(e) =>
                            edit({ background: { ...bg, from: e.target.value } }, "bg-from")
                          }
                        />
                      )}
                    </Field>
                    <Field label={t("themes.gradTo")}>
                      {(id) => (
                        <input
                          id={id}
                          type="color"
                          className="hs-input hs-input--color"
                          value={bg.to.slice(0, 7)}
                          onChange={(e) =>
                            edit({ background: { ...bg, to: e.target.value } }, "bg-to")
                          }
                        />
                      )}
                    </Field>
                    <Field label={t("themes.gradAngle")}>
                      {(id) => (
                        <TextInput
                          id={id}
                          type="number"
                          mono
                          min={0}
                          max={360}
                          value={bg.angleDeg}
                          onChange={(e) =>
                            edit(
                              { background: { ...bg, angleDeg: num(e.target.value, 0, 360, 180) } },
                              "bg-angle",
                            )
                          }
                        />
                      )}
                    </Field>
                  </>
                )}
              </div>
            </fieldset>

            <fieldset className="fieldset">
              <legend>{t("themes.font")}</legend>
              <div className="song-fields">
                <Field label={t("themes.fontFamily")} className="span-2">
                  {(id) => (
                    <Select
                      id={id}
                      value={theme.font.family}
                      onChange={(e) => edit({ font: { ...theme.font, family: e.target.value } })}
                    >
                      {[...new Set([theme.font.family, ...FONTS])].map((f) => (
                        <option key={f} value={f}>
                          {f}
                        </option>
                      ))}
                    </Select>
                  )}
                </Field>
                <Field label={t("themes.weight")}>
                  {(id) => (
                    <Select
                      id={id}
                      value={theme.font.weight}
                      onChange={(e) =>
                        edit({ font: { ...theme.font, weight: Number(e.target.value) } })
                      }
                    >
                      {[400, 500, 600, 700, 800].map((w) => (
                        <option key={w} value={w}>
                          {w}
                        </option>
                      ))}
                    </Select>
                  )}
                </Field>
                <Field label={t("themes.lineHeight")}>
                  {(id) => (
                    <TextInput
                      id={id}
                      type="number"
                      mono
                      step={0.05}
                      min={0.8}
                      max={2}
                      value={theme.font.lineHeight}
                      onChange={(e) =>
                        edit(
                          { font: { ...theme.font, lineHeight: num(e.target.value, 0.8, 2, 1.2) } },
                          "lh",
                        )
                      }
                    />
                  )}
                </Field>
                <Field label={t("themes.minPx")}>
                  {(id) => (
                    <TextInput
                      id={id}
                      type="number"
                      mono
                      min={8}
                      max={400}
                      value={theme.font.minPx}
                      onChange={(e) =>
                        edit(
                          {
                            font: {
                              ...theme.font,
                              minPx: num(e.target.value, 8, theme.font.maxPx, 36),
                            },
                          },
                          "min",
                        )
                      }
                    />
                  )}
                </Field>
                <Field label={t("themes.maxPx")}>
                  {(id) => (
                    <TextInput
                      id={id}
                      type="number"
                      mono
                      min={8}
                      max={400}
                      value={theme.font.maxPx}
                      onChange={(e) =>
                        edit(
                          {
                            font: {
                              ...theme.font,
                              maxPx: num(e.target.value, theme.font.minPx, 400, 120),
                            },
                          },
                          "max",
                        )
                      }
                    />
                  )}
                </Field>
                <Field label={t("themes.maxLines")}>
                  {(id) => (
                    <TextInput
                      id={id}
                      type="number"
                      mono
                      min={1}
                      max={12}
                      value={theme.maxLinesPerSlide}
                      onChange={(e) =>
                        edit(
                          { maxLinesPerSlide: Math.round(num(e.target.value, 1, 12, 4)) },
                          "lines",
                        )
                      }
                    />
                  )}
                </Field>
              </div>
              <SwitchRow
                label={t("themes.uniformSize")}
                checked={theme.font.uniformSize}
                onChange={(v) => edit({ font: { ...theme.font, uniformSize: v } })}
              />
              <SwitchRow
                label={t("themes.italic")}
                checked={theme.font.italic}
                onChange={(v) => edit({ font: { ...theme.font, italic: v } })}
              />
              <SwitchRow
                label={t("themes.uppercase")}
                checked={theme.font.uppercase}
                onChange={(v) => edit({ font: { ...theme.font, uppercase: v } })}
              />
              <SwitchRow
                label={t("themes.shadow")}
                checked={theme.shadow !== null}
                onChange={(v) => edit({ shadow: v ? defaultTheme.shadow : null })}
              />
            </fieldset>

            <fieldset className="fieldset">
              <legend>{t("themes.margins")}</legend>
              <div className="song-fields">
                {(["top", "right", "bottom", "left"] as const).map((side) => (
                  <Field key={side} label={t(`themes.margin.${side}`)}>
                    {(id) => (
                      <TextInput
                        id={id}
                        type="number"
                        mono
                        min={0}
                        max={40}
                        value={m[side]}
                        onChange={(e) =>
                          edit(
                            { margins: { ...m, [side]: num(e.target.value, 0, 40, 6) } },
                            `m-${side}`,
                          )
                        }
                      />
                    )}
                  </Field>
                ))}
              </div>
            </fieldset>

            <fieldset className="fieldset">
              <legend>{t("themes.transition")}</legend>
              <div className="song-fields">
                <Field label={t("themes.transitionKind")} className="span-2">
                  {(id) => (
                    <Select
                      id={id}
                      value={theme.transition.kind}
                      onChange={(e) =>
                        edit({
                          transition: {
                            ...theme.transition,
                            kind: e.target.value as TransitionKind,
                          },
                        })
                      }
                    >
                      {TRANSITIONS.map((k) => (
                        <option key={k} value={k}>
                          {t(`themes.transitions.${k}`)}
                        </option>
                      ))}
                    </Select>
                  )}
                </Field>
                <Field label={t("themes.duration")}>
                  {(id) => (
                    <TextInput
                      id={id}
                      type="number"
                      mono
                      min={0}
                      max={3000}
                      step={50}
                      value={theme.transition.durationMs}
                      onChange={(e) =>
                        edit(
                          {
                            transition: {
                              ...theme.transition,
                              durationMs: Math.round(num(e.target.value, 0, 3000, 250)),
                            },
                          },
                          "dur",
                        )
                      }
                    />
                  )}
                </Field>
              </div>
              <SwitchRow
                label={t("themes.lineByLine")}
                checked={theme.transition.lineByLine}
                onChange={(v) => edit({ transition: { ...theme.transition, lineByLine: v } })}
              />
              <SwitchRow
                label={t("themes.showCopyright")}
                checked={theme.showCopyright}
                onChange={(v) => edit({ showCopyright: v })}
              />
              <SwitchRow
                label={t("themes.showVersion")}
                checked={theme.showBibleVersionLabel}
                onChange={(v) => edit({ showBibleVersionLabel: v })}
              />
            </fieldset>
          </div>

          <section className="stack theme-preview" aria-labelledby="tp-h">
            <div className="row gap-8">
              <h2 id="tp-h" className="subhead grow">
                {t("themes.livePreview")}
              </h2>
              <Select
                aria-label={t("themes.sample")}
                value={sample}
                onChange={(e) => setSample(e.target.value as Sample)}
              >
                <option value="lyrics">{t("themes.sampleLyrics")}</option>
                <option value="bible">{t("themes.sampleBible")}</option>
              </Select>
            </div>
            <div
              className="preview-tile__screen preview-tile__screen--big"
              data-testid="theme-preview"
            >
              <SlideView slide={sampleSlide(sample, t)} theme={theme} />
            </div>
          </section>
        </div>
      </main>

      {picking && (
        <Modal
          title={t("themes.pickImageTitle")}
          closeLabel={t("common.close")}
          onClose={() => setPicking(false)}
          width={880}
        >
          <ImageSearchPanel
            onUseAsBackground={(item) => {
              edit({
                background: { kind: "image", mediaId: item.id, blurPx: 0, kenBurns: false },
              });
              setPicking(false);
            }}
          />
        </Modal>
      )}
    </div>
  );
}
