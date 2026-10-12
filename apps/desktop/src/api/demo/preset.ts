import type { HollyApi } from "@hollyshow/protocol";

/**
 * Optional start-up preset for the browser demo (`?demo=live`): the first lyric slides of the
 * sample service go live and to preview. The separate `/output` and `/stage` pages of the demo
 * each own an in-memory backend, so this is how they show something; it is also what the README
 * screenshots use. Never applied inside Tauri.
 */
export async function applyDemoPreset(api: HollyApi, search: string): Promise<void> {
  if (new URLSearchParams(search).get("demo") !== "live") return;
  await api.dispatch({ kind: "goTo", itemId: "i-santo", slideIndex: 0, target: "live" });
  await api.dispatch({ kind: "goTo", itemId: "i-santo", slideIndex: 1, target: "preview" });
}
