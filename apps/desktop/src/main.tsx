import "@hollyshow/ui";
import "./i18n";
import "./styles.css";
import "./operator/operator.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { HashRouter } from "react-router-dom";
import { ApiProvider } from "./api/context";
import { App } from "./App";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ApiProvider>
      <HashRouter>
        <App />
      </HashRouter>
    </ApiProvider>
  </StrictMode>,
);
