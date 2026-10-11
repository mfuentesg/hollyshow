import { commands, createMockApi, type StateView } from "@hollyshow/protocol";
import { Button, LiveBadge, Logo } from "@hollyshow/ui";
import { useEffect, useMemo, useState } from "react";

/**
 * Mobile remote shell. It talks to a `HollyApi`; the WebSocket adapter (pairing, reconnection,
 * resync) replaces the mock here — see docs/ARCHITECTURE.md "Adding the WebSocket adapter".
 */
export function App() {
  const api = useMemo(() => createMockApi(), []);
  const [view, setView] = useState<StateView | null>(null);

  useEffect(() => {
    void api.getSnapshot().then(({ show: _show, ...v }) => setView(v));
    return api.subscribe((u) => setView(u.view));
  }, [api]);

  if (!view) return null;
  const lines = view.live?.slide.lines ?? [];

  return (
    <div
      style={{
        display: "grid",
        gridTemplateRows: "auto 1fr auto",
        height: "100%",
        padding: "16px",
        gap: 16,
      }}
    >
      <header style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <Logo />
        <strong style={{ fontFamily: "var(--hs-font-projection)", fontSize: 20 }}>HollyShow</strong>
        <span style={{ flex: 1 }} />
        <LiveBadge />
      </header>
      <main
        style={{
          background: "var(--hs-surface)",
          border: "1px solid var(--hs-border)",
          borderRadius: "var(--hs-radius-md)",
          padding: 16,
        }}
      >
        {lines.length === 0 ? (
          <span style={{ color: "var(--hs-text-secondary)" }}>Nada en pantalla</span>
        ) : (
          lines.map((l, i) => (
            <p key={i} style={{ fontSize: 22, margin: "0 0 8px" }}>
              {l}
            </p>
          ))
        )}
      </main>
      <footer style={{ display: "grid", gridTemplateColumns: "1fr 2fr", gap: 12 }}>
        <Button onClick={() => void api.dispatch(commands.prev(view.version))}>Anterior</Button>
        <Button variant="primary" onClick={() => void api.dispatch(commands.next(view.version))}>
          Siguiente
        </Button>
        <Button
          variant="danger"
          style={{ gridColumn: "1 / -1" }}
          onClick={() => void api.dispatch(commands.blackout(!view.blackout))}
        >
          Negro
        </Button>
      </footer>
    </div>
  );
}
