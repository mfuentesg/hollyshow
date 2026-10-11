import { Navigate, Route, Routes } from "react-router-dom";
import { Operator } from "./routes/Operator";
import { Output } from "./routes/Output";
import { Stage } from "./routes/Stage";

/** One bundle, one route per window (hash routing works in every webview and dev server). */
export function App() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/operator" replace />} />
      <Route path="/operator" element={<Operator />} />
      <Route path="/output/:id" element={<Output />} />
      <Route path="/stage" element={<Stage />} />
      <Route path="*" element={<Navigate to="/operator" replace />} />
    </Routes>
  );
}
