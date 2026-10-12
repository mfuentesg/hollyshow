#!/usr/bin/env node
// Regenerates the README screenshots in docs/screenshots/.
//
//   pnpm screenshots
//
// Desktop UI: apps/desktop in a plain browser (in-memory demo backend, public-domain sample content;
// `?demo=live` puts "Santo, santo, santo" live). Phone remote: apps/remote with a fake LAN host
// injected into the page (the real one needs the Rust server). Needs a Chromium: set CHROMIUM_PATH or
// run `npx playwright-core install chromium`. Dev servers are started if not already running
// (DESKTOP_URL / REMOTE_URL override the addresses).
import { spawn } from "node:child_process";
import { existsSync, readdirSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";
import sharp from "sharp";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "docs", "screenshots");
const DESKTOP = process.env.DESKTOP_URL ?? "http://127.0.0.1:1420";
const REMOTE = process.env.REMOTE_URL ?? "http://127.0.0.1:5174";

function findChromium() {
  if (process.env.CHROMIUM_PATH) return process.env.CHROMIUM_PATH;
  try {
    const p = chromium.executablePath();
    if (existsSync(p)) return p;
  } catch {
    /* fall through */
  }
  const base = process.env.PLAYWRIGHT_BROWSERS_PATH ?? "/opt/pw-browsers";
  if (existsSync(base)) {
    for (const d of readdirSync(base).filter((n) => /^chromium-\d+$/.test(n))) {
      const p = join(base, d, "chrome-linux", "chrome");
      if (existsSync(p)) return p;
    }
  }
  throw new Error("No se encontró Chromium: define CHROMIUM_PATH");
}

async function up(url) {
  try {
    return (await fetch(url)).ok;
  } catch {
    return false;
  }
}

async function ensureServer(url, filter, children) {
  if (await up(url)) return;
  const child = spawn("pnpm", ["--filter", filter, "dev"], { cwd: root, stdio: "ignore" });
  children.push(child);
  for (let i = 0; i < 100; i++) {
    if (await up(url)) return;
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`El servidor ${filter} no arrancó en ${url}`);
}

// ---- fake LAN host for the phone remote -------------------------------------------------------

/** Runs in the page: replaces the `/ws` WebSocket with a tiny scripted HollyShow host. */
function fakeHost(cfg) {
  /* global window */ // runs in the browser
  const slide = (id, label, lines) => ({
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
  });
  const santo = [
    [
      "Verso 1",
      ["¡Santo! ¡Santo! ¡Santo! Señor omnipotente,", "siempre el labio mío loores te dará;"],
    ],
    [
      "Verso 1",
      ["¡Santo! ¡Santo! ¡Santo! te adoro reverente,", "Dios en tres personas, bendita Trinidad."],
    ],
    [
      "Verso 2",
      ["¡Santo! ¡Santo! ¡Santo! en numeroso coro,", "santos escogidos te adoran sin cesar,"],
    ],
    [
      "Verso 2",
      ["de alegría llenos, y sus coronas de oro", "rinden ante el trono y el cristalino mar."],
    ],
    ["Final", ["Dios en tres personas,", "bendita Trinidad."]],
  ].map(([l, lines], i) => slide(`i-santo:${i}`, l, lines));
  const show = [
    {
      itemId: "i-count",
      title: "Cuenta regresiva",
      durationSecs: 300,
      slides: [slide("i-count:0", null, ["5:00"])],
    },
    {
      itemId: "i-welcome",
      title: "Bienvenida",
      durationSecs: 180,
      slides: [slide("i-welcome:0", null, ["Bienvenidos", "Dios les bendiga"])],
    },
    { itemId: "i-santo", title: "Santo, santo, santo", durationSecs: 360, slides: santo },
    {
      itemId: "i-grace",
      title: "Amazing Grace",
      durationSecs: 240,
      slides: [
        slide("i-grace:0", "Verso 1", [
          "Amazing grace, how sweet the sound",
          "That saved a wretch like me",
        ]),
      ],
    },
    {
      itemId: "i-sermon",
      title: "Prédica",
      durationSecs: 2100,
      slides: [slide("i-sermon:0", null, ["Prédica"])],
    },
  ];
  const output = (kind, enabled) => ({
    kind,
    themeId: null,
    enabled,
    followsBlackout: kind === "main",
  });
  const layers = { background: null, media: null, text: true, overlay: null, blackout: false };
  const view = {
    epoch: 1,
    version: 7,
    showRevision: 1,
    live: { itemId: "i-santo", slideIndex: 0, slide: santo[0] },
    preview: { itemId: "i-santo", slideIndex: 1, slide: santo[1] },
    blackout: false,
    clear: false,
    logo: false,
    stageMessage: null,
    timers: [],
    outputs: { main: output("main", true), stage: output("stage", false) },
    outputLayers: { main: layers, stage: layers },
    itemClock: null,
    serviceMode: false,
  };

  class FakeSocket {
    constructor(url) {
      this.url = url;
      this.protocol = "";
      this.readyState = 0;
      this.listeners = {};
      setTimeout(() => {
        this.readyState = 1;
        this.onopen?.({});
      }, 20);
    }
    addEventListener(t, f) {
      (this.listeners[t] ??= []).push(f);
    }
    removeEventListener() {}
    close() {
      this.readyState = 3;
    }
    reply(msg) {
      setTimeout(() => this.onmessage?.({ data: JSON.stringify(msg) }), 10);
    }
    send(raw) {
      const msg = JSON.parse(raw);
      if (msg.type === "hello") {
        this.reply({
          type: "welcome",
          protocol: 1,
          role: cfg.role,
          token: null,
          serverName: "HollyShow · Iglesia Central",
          heartbeatMs: 5000,
          capabilities: ["bible-parallel", "timers"],
          epoch: 1,
          nowMs: Date.now(),
        });
        this.reply({
          type: "snapshot",
          snapshot: { view, show: cfg.role === "congregation" ? null : show },
          nowMs: Date.now(),
        });
      } else if (msg.type === "ping") {
        this.reply({ type: "pong", t: msg.t });
      } else if (msg.type === "query") {
        this.reply({
          type: "queryResult",
          id: msg.id,
          result: null,
          error: { code: "unsupported", message: "demo" },
        });
      }
    }
  }
  const Native = window.WebSocket;
  window.WebSocket = function (url, protocols) {
    return /\/ws$/.test(String(url)) && !protocols
      ? new FakeSocket(url)
      : new Native(url, protocols);
  };
  window.WebSocket.prototype = Native.prototype;
  Object.assign(window.WebSocket, { CONNECTING: 0, OPEN: 1, CLOSING: 2, CLOSED: 3 });
  if (cfg.role !== "congregation") {
    localStorage.setItem("hs.remote.token", "demo-token");
    localStorage.setItem("hs.remote.role", cfg.role);
  }
  localStorage.setItem("hs.remote.lang", "es");
}

// ---- capture -------------------------------------------------------------------------------------

async function save(page, name) {
  const png = await page.screenshot({ type: "png" });
  const out = await sharp(png)
    .png({ palette: true, quality: 90, effort: 10, compressionLevel: 9 })
    .toBuffer();
  writeFileSync(join(outDir, `${name}.png`), out);
  console.log(`  ${name}.png (${Math.round(out.length / 1024)} KiB)`);
}

async function shoot(browser, { size, scale = 1, url, name, setup, init }) {
  const ctx = await browser.newContext({
    viewport: size,
    deviceScaleFactor: scale,
    locale: "es-ES",
    reducedMotion: "reduce",
  });
  const page = await ctx.newPage();
  if (init) await page.addInitScript(init.fn, init.arg);
  await page.goto(url);
  await page.waitForTimeout(1200);
  await setup?.(page);
  await page.waitForTimeout(500);
  await save(page, name);
  await ctx.close();
}

async function main() {
  mkdirSync(outDir, { recursive: true });
  const children = [];
  try {
    await ensureServer(DESKTOP, "desktop", children);
    await ensureServer(REMOTE, "remote", children);
    const browser = await chromium.launch({ executablePath: findChromium() });
    const D = { width: 1440, height: 900 };
    const S = { width: 1280, height: 720 };
    const P = { width: 390, height: 844 };
    const live = `${DESKTOP}/?demo=live#`;

    await shoot(browser, { size: D, url: `${live}/operator`, name: "operador" });
    await shoot(browser, {
      size: D,
      url: `${DESKTOP}/#/operator/bible`,
      name: "biblia-multiversion",
    });
    await shoot(browser, {
      size: D,
      url: `${DESKTOP}/#/operator/songs/song-santo`,
      name: "editor-cancion",
    });
    await shoot(browser, {
      size: D,
      url: `${DESKTOP}/#/operator/themes`,
      name: "buscar-fondos",
      setup: async (page) => {
        await page
          .locator("select")
          .filter({ has: page.locator('option[value="image"]') })
          .first()
          .selectOption("image");
        await page.getByPlaceholder(/cruz, amanecer/i).fill("cruz");
        await page.getByPlaceholder(/cruz, amanecer/i).press("Enter");
        await page.waitForTimeout(800);
      },
    });
    await shoot(browser, {
      size: D,
      url: `${DESKTOP}/#/operator/settings/remote`,
      name: "ajustes-remoto",
      setup: (page) =>
        page
          .getByText("Emparejar un dispositivo")
          .evaluate((el) => el.scrollIntoView({ block: "center" })),
    });
    await shoot(browser, { size: S, url: `${live}/output/main`, name: "salida" });
    await shoot(browser, { size: S, url: `${live}/stage`, name: "escenario" });

    await shoot(browser, {
      size: P,
      scale: 2,
      url: `${REMOTE}/`,
      name: "remoto-operador",
      init: { fn: fakeHost, arg: { role: "operator" } },
    });
    await shoot(browser, {
      size: P,
      scale: 2,
      url: `${REMOTE}/?view=congregation`,
      name: "remoto-congregacion",
      init: { fn: fakeHost, arg: { role: "congregation" } },
    });
    await browser.close();
  } finally {
    for (const c of children) c.kill();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
