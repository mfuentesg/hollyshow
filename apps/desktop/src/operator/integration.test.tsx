import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiProvider } from "../api/context";
import { createDemoBackend } from "../api/demo/backend";
import type { StartupInfo } from "../api/host";
import { App } from "../App";
import i18n from "../i18n";
import { renderApp } from "../test-utils";

beforeEach(async () => {
  await i18n.changeLanguage("es");
  window.localStorage.clear();
});

function renderWithStartup(info: StartupInfo) {
  const backend = createDemoBackend();
  const host = { ...backend.host, startupInfo: async () => info };
  return render(
    <ApiProvider api={backend.api} host={host}>
      <MemoryRouter initialEntries={["/operator"]}>
        <App />
      </MemoryRouter>
    </ApiProvider>,
  );
}

describe("host integration in the operator window", () => {
  it("tells the operator the previous service was resumed", async () => {
    renderWithStartup({
      resumed: true,
      crashed: false,
      playlist: "Culto dominical",
      hadLive: true,
      dbRecovered: false,
    });
    expect(
      await screen.findByText(/Retomamos donde estabas: Culto dominical, con una diapositiva/),
    ).toBeInTheDocument();
  });

  it("explains a crash recovery and a restored library", async () => {
    renderWithStartup({
      resumed: false,
      crashed: true,
      playlist: null,
      hadLive: false,
      dbRecovered: true,
    });
    expect(await screen.findByText(/se cerró de forma inesperada/)).toBeInTheDocument();
    expect(await screen.findByText(/restauró desde una copia de seguridad/)).toBeInTheDocument();
  });

  it("stays quiet on a normal start", async () => {
    renderApp("/operator");
    await screen.findByRole("navigation", { name: /secciones/i });
    expect(screen.queryByText(/Retomamos/)).not.toBeInTheDocument();
  });

  it("offers an Updates section in settings", async () => {
    renderApp("/operator/settings/updates");
    expect(await screen.findByRole("heading", { name: "Actualizaciones" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Buscar actualizaciones/ })).toBeInTheDocument();
  });

  it("opens the image bank from the theme editor", async () => {
    const user = userEvent.setup();
    renderApp("/operator/themes");
    const kind = await screen.findByLabelText("Tipo de fondo");
    await user.selectOptions(kind, "image");
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getAllByText(/Buscar fondos/).length).toBeGreaterThan(0);
  });

  it("lets the operator choose the role a PIN grants", async () => {
    const user = userEvent.setup();
    const backend = createDemoBackend();
    const spy = vi.spyOn(backend.host, "setPairingRole");
    render(
      <ApiProvider api={backend.api} host={backend.host}>
        <MemoryRouter initialEntries={["/operator/settings/remote"]}>
          <App />
        </MemoryRouter>
      </ApiProvider>,
    );
    const select = await screen.findByLabelText("Rol que concede el PIN");
    await user.selectOptions(select, "pastor");
    await waitFor(() => expect(spy).toHaveBeenCalledWith("pastor"));
  });
});
