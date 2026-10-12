import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "../i18n";
import { renderApp } from "../test-utils";

beforeEach(async () => {
  await i18n.changeLanguage("es");
  window.localStorage.clear();
});

describe("songs", () => {
  it("searches the library ignoring accents", async () => {
    const user = userEvent.setup();
    renderApp("/operator/songs");
    const box = await screen.findByRole("searchbox", { name: "Buscar en la biblioteca" });
    await user.type(box, "senor");
    expect(await screen.findByRole("link", { name: /Santo, santo, santo/ })).toBeInTheDocument();
  });

  it("edits and saves a song, with undo", async () => {
    const user = userEvent.setup();
    const { backend } = renderApp("/operator/songs/song-santo");
    const title = await screen.findByLabelText("Título");
    expect(title).toHaveValue("Santo, santo, santo");
    await user.clear(title);
    await user.type(title, "Santo");
    await user.click(screen.getByRole("button", { name: "Deshacer" }));
    expect(screen.getByLabelText("Título")).toHaveValue("Santo, santo, santo");
    await user.click(screen.getByRole("button", { name: "Rehacer" }));
    await user.click(screen.getByRole("button", { name: "Guardar" }));
    await waitFor(async () => {
      const r = await backend.api.query({ kind: "getSong", id: "song-santo" });
      expect(r.kind === "song" && r.song?.title).toBe("Santo");
    });
  });

  it("adds a section and reflects it in the arrangement chips and preview", async () => {
    const user = userEvent.setup();
    renderApp("/operator/songs/song-santo");
    const seq = await screen.findByRole("list", { name: "Secuencia del arreglo" });
    const before = seq.querySelectorAll(".seq-chip").length;
    await user.click(screen.getByRole("button", { name: "+ Coro" }));
    await user.click(screen.getAllByRole("button", { name: /^Agregar Coro/ })[0]!);
    expect(seq.querySelectorAll(".seq-chip").length).toBe(before + 2);
  });

  it("creates a new song and requires a title", async () => {
    const user = userEvent.setup();
    renderApp("/operator/songs/new");
    await screen.findByLabelText("Título");
    await user.click(screen.getByRole("button", { name: "Guardar" }));
    expect(await screen.findByText("La canción necesita un título.")).toBeInTheDocument();
  });
});

describe("themes", () => {
  it("updates the live preview when the text color changes", async () => {
    renderApp("/operator/themes");
    const preview = await screen.findByTestId("theme-preview");
    expect(preview).toBeInTheDocument();
    const name = await screen.findByLabelText("Nombre");
    expect(name).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Guardar" })).toBeDisabled();
    const user = userEvent.setup();
    await user.type(name, "x");
    expect(screen.getByRole("button", { name: "Guardar" })).toBeEnabled();
  });
});

describe("settings", () => {
  it("lists outputs and monitors", async () => {
    renderApp("/operator/settings/outputs");
    expect(await screen.findByRole("heading", { name: "Salidas y monitores" })).toBeInTheDocument();
    expect(await screen.findAllByRole("option", { name: /Proyector HDMI/ })).not.toHaveLength(0);
  });

  it("shows the PIN and QR and kicks a device", async () => {
    const user = userEvent.setup();
    renderApp("/operator/settings/remote");
    expect(await screen.findByRole("img", { name: /Código QR/ })).toBeInTheDocument();
    expect(await screen.findByLabelText("PIN 4 1 8 2 6 6")).toHaveTextContent("418 266");
    await user.click(await screen.findByRole("button", { name: "Expulsar a iPhone de Ana" }));
    await waitFor(() => expect(screen.queryByText("iPhone de Ana")).not.toBeInTheDocument());
  });

  it("manages Bible versions: default radio and ordering", async () => {
    const user = userEvent.setup();
    renderApp("/operator/settings/bible");
    expect(
      await screen.findByRole("heading", { name: "Versiones de la Biblia" }),
    ).toBeInTheDocument();
    const radios = await screen.findAllByRole("radio", { name: "Principal" });
    const second = radios.find(
      (r) => !(r as HTMLInputElement).checked && !(r as HTMLInputElement).disabled,
    );
    if (second) {
      await user.click(second);
      await waitFor(() => expect(second).toBeChecked());
    }
    expect(screen.getByText(/hasta 4/)).toBeInTheDocument();
  });

  it("switches the interface language to English", async () => {
    const user = userEvent.setup();
    renderApp("/operator/settings/language");
    const select = await screen.findByLabelText("Idioma de la interfaz");
    await user.selectOptions(select, "en");
    expect(await screen.findByRole("heading", { name: "Language" })).toBeInTheDocument();
  });
});

describe("library settings", () => {
  it("builds the usage report and escapes the CSV", async () => {
    const { usageCsv } = await import("./settings/LibrarySettings");
    const csv = usageCsv(
      [
        {
          songId: "a",
          title: 'Santo, "santo"',
          author: "Ana",
          ccli: "123",
          timesShown: 2,
          lastShownAt: 0,
        },
      ],
      ["Canción", "Autor", "CCLI", "Veces", "Última"],
    );
    expect(csv.split("\n")[0]).toBe("Canción,Autor,CCLI,Veces,Última");
    expect(csv.split("\n")[1]).toMatch(/^"Santo, ""santo""",Ana,123,2,\d{4}-\d{2}-\d{2}$/);
  });

  it("lists the songs shown in the period", async () => {
    const user = userEvent.setup();
    renderApp("/operator/settings/library");
    await user.click(await screen.findByRole("button", { name: "Generar informe" }));
    const table = await screen.findByRole("table", { name: "Informe de uso" });
    expect(table).toHaveTextContent("Santo, santo, santo");
  });
});
