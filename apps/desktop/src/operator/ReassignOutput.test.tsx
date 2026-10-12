import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiProvider } from "../api/context";
import { createDemoBackend } from "../api/demo/backend";
import { ToastProvider, useToasts } from "../hooks/useToasts";
import i18n from "../i18n";
import { ReassignOutput } from "./ReassignOutput";

beforeEach(async () => {
  await i18n.changeLanguage("es");
});

function PausedToast({ outputs }: { outputs: string[] }) {
  const toasts = useToasts();
  return (
    <button
      onClick={() =>
        toasts.action("Salida en pausa", (dismiss) => (
          <ReassignOutput outputs={outputs} onDone={dismiss} />
        ))
      }
    >
      lanzar
    </button>
  );
}

function setup(outputs = ["main"]) {
  const backend = createDemoBackend();
  const open = vi.spyOn(backend.host, "openOutput");
  render(
    <ApiProvider api={backend.api} host={backend.host}>
      <ToastProvider>
        <PausedToast outputs={outputs} />
      </ToastProvider>
    </ApiProvider>,
  );
  return { backend, open };
}

describe("ReassignOutput", () => {
  it("lists this screen and the available monitors, then reassigns the output", async () => {
    const user = userEvent.setup();
    const { open, backend } = setup();
    await user.click(screen.getByText("lanzar"));
    await user.click(await screen.findByRole("button", { name: "Reasignar" }));
    const menu = await screen.findByRole("menu", { name: "Elegir pantalla para la salida" });
    const items = await screen.findAllByRole("menuitem");
    expect(menu).toBeInTheDocument();
    expect(items.map((i) => i.textContent)).toEqual([
      "Esta pantalla",
      expect.stringContaining("Pantalla integrada"),
      expect.stringContaining("Proyector HDMI"),
    ]);
    await user.click(items[2]!);
    await waitFor(() => expect(open).toHaveBeenCalledWith("main", 1));
    expect(await backend.host.outputMonitors()).toEqual({ main: 1 });
    // The paused toast is replaced by a confirmation.
    expect(await screen.findByText("Salida movida a la pantalla 2.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Reasignar" })).not.toBeInTheDocument();
  });

  it("'Esta pantalla' targets the operator's own monitor", async () => {
    const user = userEvent.setup();
    const { open } = setup();
    await user.click(screen.getByText("lanzar"));
    await user.click(await screen.findByRole("button", { name: "Reasignar" }));
    await user.click(await screen.findByRole("menuitem", { name: "Esta pantalla" }));
    await waitFor(() => expect(open).toHaveBeenCalledWith("main", 0));
  });

  it("reassigns every paused output and renders nothing without outputs", async () => {
    const user = userEvent.setup();
    const { open } = setup(["main", "aux1"]);
    await user.click(screen.getByText("lanzar"));
    await user.click(await screen.findByRole("button", { name: "Reasignar" }));
    await user.click((await screen.findAllByRole("menuitem"))[2]!);
    await waitFor(() => expect(open).toHaveBeenCalledTimes(2));
    expect(open).toHaveBeenCalledWith("aux1", 1);
  });

  it("renders nothing when no output is paused", () => {
    const backend = createDemoBackend();
    const { container } = render(
      <ApiProvider api={backend.api} host={backend.host}>
        <ToastProvider>
          <ReassignOutput outputs={[]} onDone={() => {}} />
        </ToastProvider>
      </ApiProvider>,
    );
    expect(container.querySelector(".toast__actions")).toBeNull();
  });
});
