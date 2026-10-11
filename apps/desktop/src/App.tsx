import { Navigate, Route, Routes } from "react-router-dom";
import { BiblePage } from "./operator/BiblePage";
import { OperatorLayout } from "./operator/OperatorLayout";
import { ServicePage } from "./operator/ServicePage";
import { BibleSettings } from "./operator/settings/BibleSettings";
import { LanguageSettings } from "./operator/settings/LanguageSettings";
import { OutputsSettings } from "./operator/settings/OutputsSettings";
import { RemoteSettings } from "./operator/settings/RemoteSettings";
import { SettingsLayout } from "./operator/settings/SettingsLayout";
import { SongsPage } from "./operator/songs/SongsPage";
import { ThemesPage } from "./operator/themes/ThemesPage";
import { Output } from "./routes/Output";
import { Stage } from "./routes/Stage";

/** One bundle, one route per window (hash routing works in every webview and dev server). */
export function App() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/operator" replace />} />
      <Route path="/operator" element={<OperatorLayout />}>
        <Route index element={<ServicePage />} />
        <Route path="songs" element={<SongsPage />} />
        <Route path="songs/:id" element={<SongsPage />} />
        <Route path="bible" element={<BiblePage />} />
        <Route path="themes" element={<ThemesPage />} />
        <Route path="settings" element={<SettingsLayout />}>
          <Route index element={<Navigate to="outputs" replace />} />
          <Route path="outputs" element={<OutputsSettings />} />
          <Route path="bible" element={<BibleSettings />} />
          <Route path="remote" element={<RemoteSettings />} />
          <Route path="language" element={<LanguageSettings />} />
        </Route>
      </Route>
      <Route path="/output/:id" element={<Output />} />
      <Route path="/stage" element={<Stage />} />
      <Route path="*" element={<Navigate to="/operator" replace />} />
    </Routes>
  );
}
