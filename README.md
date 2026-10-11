# HollyShow

Presentación para iglesias (letras, Biblia, medios) con control remoto por red local.
Tauri 2 + React + Rust. Plan: `docs/PLAN.md`. Arquitectura: `docs/ARCHITECTURE.md`. Diseño: `docs/design/`.

## Requisitos

Rust estable, Node 22, pnpm 10, y las dependencias de Tauri 2 de tu SO
(Linux: `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev`).

## Comandos

```sh
pnpm bootstrap      # primera vez en un clon: instala y construye los frontends (los crates los incrustan)
pnpm dev            # app de escritorio (Tauri + Vite)
pnpm dev:web        # solo UI en el navegador con datos de demostración
pnpm dev:remote     # app remota móvil
pnpm gen:types      # regenera los tipos TS desde Rust (ts-rs); commitea packages/protocol/src/generated
pnpm check:version  # Cargo.toml, package.json y tag coherentes (el release lo exige)
pnpm lint && pnpm typecheck && pnpm test
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm build          # instaladores del SO actual (Linux: .deb y .AppImage en target/release/bundle)
```

Lanzamientos: etiqueta `v*` -> workflow `Release` (borrador de release con instaladores sin firmar).

## Datos del usuario y primera ejecución

La biblioteca (SQLite) vive en el directorio de datos del SO (`~/.local/share/com.hollyshow.app` en Linux,
`~/Library/Application Support/com.hollyshow.app` en macOS, `%APPDATA%\com.hollyshow.app` en Windows) junto con `backups/`
(copia diaria, 14 días) y `media/` (imágenes descargadas). En el primer arranque se instalan las Biblias de dominio público
incluidas (`apps/desktop/src-tauri/resources/bibles/SOURCES.md`) y se crean un tema y una plantilla de culto. El servidor de
control remoto escucha en el puerto 7420 (`remote.port` en ajustes) y se anuncia como `hollyshow.local`.

En Linux con Wayland, HollyShow usa XWayland cuando existe (`GDK_BACKEND=x11`) porque Wayland no deja que una aplicación
elija en qué monitor abre una ventana; define `GDK_BACKEND` tú mismo para cambiarlo.
