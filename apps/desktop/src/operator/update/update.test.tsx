import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { ToastProvider } from "../../hooks/useToasts";
import { Markdown, parseBlocks } from "./markdown";
import { applyEvent, percent, releaseUrl } from "./state";
import type { ReleaseInfo, UpdateState } from "./types";
import { UpdateProvider } from "./UpdateContext";
import { UpdateFooterIndicator, UpdateOverlay, UpdateSettings } from "./UpdateUi";
import { createMemoryUpdateApi } from "./updateApi";
import "./index";

const REL: ReleaseInfo = {
  version: "0.2.0",
  notes: "## Novedades\n\n- Mejor **rendimiento**\n- Corrige [un error](https://example.com/x)\n",
  date: Date.UTC(2026, 5, 1),
};

beforeEach(async () => {
  await i18n.changeLanguage("es");
});

function setup(opts: Parameters<typeof createMemoryUpdateApi>[0] = {}, busy = false) {
  const mem = createMemoryUpdateApi({ currentVersion: "0.1.0", release: REL, ...opts });
  render(
    <ToastProvider>
      <UpdateProvider api={mem.api} busy={busy}>
        <UpdateFooterIndicator />
        <UpdateOverlay />
        <UpdateSettings />
      </UpdateProvider>
    </ToastProvider>,
  );
  return mem;
}

describe("update state", () => {
  it("folds events and never hides a ready update", () => {
    let s: UpdateState = { state: "idle" };
    s = applyEvent(s, { type: "available", info: REL });
    s = applyEvent(s, { type: "progress", version: "0.2.0", downloaded: 50, total: 200 });
    expect(s).toMatchObject({ state: "downloading", downloaded: 50, notes: REL.notes });
    s = applyEvent(s, { type: "ready", info: REL, deferred: false });
    expect(s.state).toBe("ready");
    expect(applyEvent(s, { type: "checking" }).state).toBe("ready");
    expect(applyEvent(s, { type: "none" }).state).toBe("ready");
    expect(applyEvent(s, { type: "error", message: "x" }).state).toBe("error");
  });

  it("computes percentages", () => {
    expect(percent(50, 200)).toBe(25);
    expect(percent(5, null)).toBeNull();
    expect(percent(500, 200)).toBe(100);
  });
});

describe("markdown", () => {
  it("renders structure and drops raw HTML, images and unsafe links", () => {
    const src = [
      "# Título",
      "",
      "<script>alert(1)</script> <img src=x onerror=alert(1)>",
      "",
      "- uno",
      "- dos con `código`",
      "",
      "[mal](javascript:alert(1)) ![img](https://e.com/a.png) https://ok.example/p.",
    ].join("\n");
    const { container } = render(<Markdown source={src} />);
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
    expect(container.textContent).toContain("<script>alert(1)</script>");
    expect(container.querySelector("h3")?.textContent).toBe("Título");
    expect(container.querySelectorAll("li")).toHaveLength(2);
    const links = [...container.querySelectorAll("a")];
    expect(links.map((a) => a.getAttribute("href"))).toEqual(["https://ok.example/p"]);
    expect(links[0]?.getAttribute("rel")).toContain("noopener");
    expect(container.textContent).toContain("mal");
  });

  it("opens links in the system browser without navigating the webview", async () => {
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    const user = userEvent.setup();
    render(<Markdown source="[ver](https://example.com/x) [http](http://example.com/y)" />);
    expect(screen.queryByRole("link", { name: "http" })).toBeNull();
    await user.click(screen.getByRole("link", { name: "ver" }));
    expect(open).toHaveBeenCalledWith("https://example.com/x", "_blank", "noopener,noreferrer");
    open.mockRestore();
  });

  it("parses fenced code verbatim", () => {
    expect(parseBlocks("```\n<b>x</b>\n```")).toEqual([{ t: "code", text: "<b>x</b>" }]);
  });
});

describe("update UI", () => {
  it("manual check downloads and shows the banner with all actions", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("button", { name: "Buscar actualizaciones…" }));
    const banner = await screen.findByRole("status", { name: "Aviso de actualización" });
    expect(banner).toHaveTextContent("HollyShow 0.2.0 está lista");
    expect(screen.getByRole("button", { name: "Reiniciar y actualizar" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Ver novedades" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Mejor rendimiento");
    expect(dialog.querySelector('a[href="https://example.com/x"]')).not.toBeNull();
    const gh = within(dialog).getByRole("link", { name: "Ver en GitHub" });
    expect(gh).toHaveAttribute(
      "href",
      "https://github.com/mfuentesg/hollyshow/releases/tag/v0.2.0",
    );
    expect(gh).toHaveAttribute("target", "_blank");
  });

  it("builds the release page URL from the version", () => {
    expect(releaseUrl("0.2.0")).toBe("https://github.com/mfuentesg/hollyshow/releases/tag/v0.2.0");
    expect(releaseUrl("v1.0.0-beta.1")).toBe(
      "https://github.com/mfuentesg/hollyshow/releases/tag/v1.0.0-beta.1",
    );
  });

  it("'Más tarde' hides the banner but the footer keeps the pending update", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("button", { name: "Buscar actualizaciones…" }));
    await user.click(await screen.findByRole("button", { name: "Más tarde" }));
    expect(screen.queryByRole("status", { name: "Aviso de actualización" })).toBeNull();
    expect(screen.getByRole("button", { name: "HollyShow 0.2.0 está lista" })).toBeInTheDocument();
  });

  it("restarts when idle", async () => {
    const user = userEvent.setup();
    const mem = setup();
    await user.click(screen.getByRole("button", { name: "Buscar actualizaciones…" }));
    await user.click(await screen.findByRole("button", { name: "Reiniciar y actualizar" }));
    await waitFor(() => expect(mem.state.installed).toBe(true));
  });

  it("never offers an immediate restart while busy: installs on exit", async () => {
    const user = userEvent.setup();
    const mem = setup({ busy: true }, true);
    await user.click(screen.getByRole("button", { name: "Buscar actualizaciones…" }));
    const banner = await screen.findByRole("status", { name: "Aviso de actualización" });
    expect(banner).toHaveTextContent("Se instalará al salir");
    expect(screen.queryByRole("button", { name: "Reiniciar y actualizar" })).toBeNull();
    expect(mem.state.installed).toBe(false);
  });

  it("falls back to install-on-exit when the backend refuses with busy", async () => {
    const user = userEvent.setup();
    const mem = setup();
    await user.click(screen.getByRole("button", { name: "Buscar actualizaciones…" }));
    const restart = await screen.findByRole("button", { name: "Reiniciar y actualizar" });
    mem.state.busy = true; // service mode started after the banner appeared
    await user.click(restart);
    expect(await screen.findByText(/se aplicará al cerrar la aplicación/)).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "Aviso de actualización" })).toHaveTextContent(
      "Se instalará al salir",
    );
    expect(mem.state.installed).toBe(false);
  });

  it("shows download progress in the footer", async () => {
    setup();
    const mem = setup();
    await act(async () => {
      await Promise.resolve();
      mem.emit({ type: "progress", version: "0.2.0", downloaded: 25, total: 100 });
    });
    expect((await screen.findAllByText(/Descargando 0\.2\.0 · 25%/)).length).toBeGreaterThan(0);
    expect(screen.getAllByRole("progressbar", { name: "Progreso de la descarga" })[0]).toHaveValue(
      25,
    );
  });

  it("reports an up to date app", async () => {
    const user = userEvent.setup();
    setup({ release: null });
    await user.click(screen.getByRole("button", { name: "Buscar actualizaciones…" }));
    expect(await screen.findByText("HollyShow está al día (0.1.0)")).toBeInTheDocument();
  });

  it("persists settings through the api", async () => {
    const user = userEvent.setup();
    const mem = setup();
    await user.click(
      screen.getByRole("checkbox", { name: "Buscar actualizaciones automáticamente" }),
    );
    await user.selectOptions(screen.getByLabelText("Canal"), "beta");
    expect(await mem.api.getSettings()).toEqual({
      autoCheck: false,
      autoDownload: true,
      channel: "beta",
    });
    expect(screen.getByText(/puede tener errores/)).toBeInTheDocument();
  });

  it("is available in English", async () => {
    await i18n.changeLanguage("en");
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("button", { name: "Check for updates…" }));
    expect(await screen.findByRole("status", { name: "Update notice" })).toHaveTextContent(
      "HollyShow 0.2.0 is ready",
    );
  });
});
