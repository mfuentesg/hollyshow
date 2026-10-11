import { render } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { ApiProvider } from "./api/context";
import { createDemoBackend, type DemoSeed } from "./api/demo/backend";
import { App } from "./App";
import "./i18n";

/** Renders the whole app on `path` against an in-memory demo backend (no Tauri). */
export function renderApp(path = "/operator", seed: DemoSeed = {}) {
  const backend = createDemoBackend(seed);
  const utils = render(
    <ApiProvider api={backend.api} host={backend.host}>
      <MemoryRouter initialEntries={[path]}>
        <App />
      </MemoryRouter>
    </ApiProvider>,
  );
  return { backend, ...utils };
}
