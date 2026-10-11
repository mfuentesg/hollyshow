# HollyShow

Presentación para iglesias (letras, Biblia, medios) con control remoto por red local.
Tauri 2 + React + Rust. Plan: `docs/PLAN.md`. Arquitectura: `docs/ARCHITECTURE.md`. Diseño: `docs/design/`.

## Requisitos

Rust estable, Node 22, pnpm 10, y las dependencias de Tauri 2 de tu SO
(Linux: `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev`).

## Comandos

```sh
pnpm install
pnpm dev            # app de escritorio (Tauri + Vite)
pnpm dev:web        # solo UI en el navegador con datos de demostración
pnpm dev:remote     # app remota móvil
pnpm gen:types      # regenera los tipos TS desde Rust (ts-rs)
pnpm lint && pnpm typecheck && pnpm test
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm build          # instaladores del SO actual
```

Lanzamientos: etiqueta `v*` -> workflow `Release` (borrador de release con instaladores sin firmar).
