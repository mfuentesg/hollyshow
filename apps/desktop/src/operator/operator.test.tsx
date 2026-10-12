import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "../i18n";
import { renderApp } from "../test-utils";

beforeEach(async () => {
  await i18n.changeLanguage("es");
  window.localStorage.clear();
});

describe("operator: service screen", () => {
  it("shows the order, header status and footer health", async () => {
    renderApp();
    expect(
      await screen.findByRole("complementary", { name: "Orden del culto" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Modo culto/ })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(await screen.findByText(/Salida 60 fps/)).toBeInTheDocument();
    expect(screen.getByText(/PIN/)).toBeInTheDocument();
  });

  it("selects an item, previews a slide and sends it live with Enter", async () => {
    const user = userEvent.setup();
    const { backend } = renderApp();
    const order = await screen.findByRole("list", { name: "Orden del culto" });
    await user.click(await within(order).findByText("Santo, santo, santo"));
    const tiles = await screen.findAllByRole("button", { name: /^Diapositiva 1,/ });
    await user.click(tiles[0]!);
    await waitFor(() => expect(backend.state.preview).not.toBeNull());
    expect(backend.state.live).toBeNull();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(backend.state.live).not.toBeNull());
  });

  it("toggles black with the B key and the panic button reflects it", async () => {
    const user = userEvent.setup();
    const { backend } = renderApp();
    const black = await screen.findByRole("button", { name: "Negro" });
    await user.keyboard("b");
    await waitFor(() => expect(backend.state.blackout).toBe(true));
    await waitFor(() => expect(black).toHaveAttribute("aria-pressed", "true"));
    await user.keyboard("b");
    await waitFor(() => expect(backend.state.blackout).toBe(false));
  });

  it("reorders items with the move buttons and supports undo", async () => {
    const user = userEvent.setup();
    renderApp();
    const order = await screen.findByRole("list", { name: "Orden del culto" });
    const titles = () =>
      within(order)
        .getAllByRole("listitem")
        .map((li) => li.querySelector(".order-item__title")?.textContent);
    const before = titles();
    await user.click(within(order).getByRole("button", { name: `Bajar ${before[0]!}` }));
    await waitFor(() => expect(titles()[1]).toBe(before[0]));
    await user.click(screen.getByRole("button", { name: "Deshacer" }));
    await waitFor(() => expect(titles()).toEqual(before));
  });

  it("opens the command palette with Ctrl+K and finds a song without accents", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("complementary", { name: "Orden del culto" });
    await user.keyboard("{Control>}k{/Control}");
    const input = await screen.findByRole("combobox", { name: "Paleta de comandos" });
    await user.type(input, "senor");
    expect(await screen.findByRole("option", { name: /Santo, santo, santo/ })).toBeInTheDocument();
  });

  it("palette parses a Bible reference and previews it", async () => {
    const user = userEvent.setup();
    const { backend } = renderApp();
    await screen.findByRole("complementary", { name: "Orden del culto" });
    await user.keyboard("{Control>}k{/Control}");
    const input = await screen.findByRole("combobox", { name: "Paleta de comandos" });
    await user.type(input, "Jn 3:16");
    await screen.findAllByRole("option", { name: /Juan 3:16/ });
    await user.keyboard("{Enter}");
    await waitFor(() => expect(backend.state.preview?.slide.kind).toBe("bible"));
  });

  it("toggles service mode from the header", async () => {
    const user = userEvent.setup();
    const { backend } = renderApp();
    const btn = await screen.findByRole("button", { name: /Modo culto/ });
    await user.click(btn);
    await waitFor(() => expect(backend.state.serviceMode).toBe(true));
    await act(async () => {});
    expect(btn).toHaveAttribute("aria-pressed", "true");
  });
});
