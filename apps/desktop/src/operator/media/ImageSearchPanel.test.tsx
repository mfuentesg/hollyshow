import type { MediaLibraryItem } from "@hollyshow/protocol";
import { createMockApi } from "@hollyshow/protocol";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiProvider } from "../../api/context";
import { createDemoBackend } from "../../api/demo/backend";
import i18n from "../../i18n";
import { ImageSearchPanel } from "./ImageSearchPanel";
import { licenseTone } from "./license";

beforeEach(async () => {
  await i18n.changeLanguage("es");
});

function setup(onUse?: (i: MediaLibraryItem) => void) {
  const backend = createDemoBackend();
  const api = createMockApi();
  render(
    <ApiProvider api={api} host={backend.host}>
      <ImageSearchPanel onUseAsBackground={onUse} />
    </ApiProvider>,
  );
  return api;
}

describe("licenseTone", () => {
  it("classifies badges", () => {
    expect(licenseTone("CC0 1.0")).toBe("free");
    expect(licenseTone("PDM")).toBe("free");
    expect(licenseTone("CC BY-SA 4.0")).toBe("attrib");
    expect(licenseTone("CC BY-NC 4.0")).toBe("restricted");
  });
});

describe("ImageSearchPanel", () => {
  it("shows the curated categories and searches from a chip", async () => {
    const user = userEvent.setup();
    setup();
    const chips = await screen.findByRole("list", { name: "Categorías" });
    await user.click(within(chips).getByRole("button", { name: "Cruz" }));
    expect(screen.getByRole("searchbox", { name: "Buscar imágenes" })).toHaveValue("cruz");
    const grid = await screen.findByRole("list", { name: "Resultados de imágenes" });
    expect(within(grid).getByRole("button", { name: /Cruz de madera/ })).toBeInTheDocument();
    expect(within(grid).getByText("CC BY 4.0")).toBeInTheDocument();
  });

  it("previews with attribution, downloads, and uses as background", async () => {
    const user = userEvent.setup();
    const onUse = vi.fn();
    const api = setup(onUse);
    await user.type(
      await screen.findByRole("searchbox", { name: "Buscar imágenes" }),
      "vitral{enter}",
    );
    await user.click(await screen.findByRole("button", { name: /Vitral de catedral/ }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("María Pérez")).toBeInTheDocument();
    expect(within(dialog).getByText(/is licensed under CC BY-SA 4.0/)).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Descargar" }));
    await waitFor(async () => expect(await api.mediaList?.()).toHaveLength(1));
    await user.click(
      await within(screen.getByRole("dialog")).findByRole("button", { name: "Usar como fondo" }),
    );
    expect(onUse).toHaveBeenCalledOnce();
    expect(onUse.mock.calls[0]![0].id).toBe("commons-m3");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("lists downloaded images and removes them", async () => {
    const user = userEvent.setup();
    const api = createMockApi();
    const found = await api.mediaSearch!({ query: "lago" });
    await api.mediaDownload!(found.results[0]!);
    const backend = createDemoBackend();
    render(
      <ApiProvider api={api} host={backend.host}>
        <ImageSearchPanel />
      </ApiProvider>,
    );
    await user.click(await screen.findByRole("button", { name: /Descargadas \(1\)/ }));
    const grid = screen.getByRole("list", { name: "Imágenes descargadas" });
    expect(within(grid).getByAltText("Lago en la montaña")).toBeInTheDocument();
    await user.click(within(grid).getByRole("button", { name: "Eliminar Lago en la montaña" }));
    expect(await screen.findByText("Aún no has descargado imágenes.")).toBeInTheDocument();
    expect(await api.mediaList!()).toHaveLength(0);
  });

  it("reports rate limiting per source", async () => {
    const api = createMockApi();
    api.mediaSearch = async () => ({
      results: [],
      page: 1,
      hasMore: false,
      translatedQuery: "cross",
      failures: [{ source: "openverse", message: "429", retryAfterSecs: 12 }],
    });
    const backend = createDemoBackend();
    const user = userEvent.setup();
    render(
      <ApiProvider api={api} host={backend.host}>
        <ImageSearchPanel />
      </ApiProvider>,
    );
    await user.type(screen.getByRole("searchbox", { name: "Buscar imágenes" }), "cruz{enter}");
    expect(
      await screen.findByText("Openverse pidió esperar. Reintenta en 12 s."),
    ).toBeInTheDocument();
  });

  it("degrades when the host has no image bank", () => {
    const backend = createDemoBackend();
    const api = createMockApi();
    delete (api as { mediaSearch?: unknown }).mediaSearch;
    render(
      <ApiProvider api={api} host={backend.host}>
        <ImageSearchPanel />
      </ApiProvider>,
    );
    expect(
      screen.getByText("El banco de imágenes no está disponible en esta versión."),
    ).toBeInTheDocument();
  });
});
