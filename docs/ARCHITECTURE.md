# HollyShow: arquitectura de implementación

Complementa `docs/PLAN.md` (qué y por qué) con el cómo del código. Fuente de verdad visual: `docs/design/`.
El código manda sobre este documento: si divergen, corrige el documento en el mismo commit.

## 1. Mapa de módulos

```
apps/desktop/            Tauri 2 + React (operador, salida, escenario: una SPA, una ruta por ventana)
  src/                   routes/{Operator,Output,Stage}.tsx, api/tauri.ts (adaptador HollyApi), i18n/
  src-tauri/             wiring: IPC commands, ventanas, puente motor -> webviews
apps/remote/             SPA móvil (mobile-first); su dist/ se embebe en crates/server (rust-embed)
crates/core/             dominio puro: tipos, Command/Event, reductor, Engine, protocolo de cable
crates/storage/          traits de repositorio + MemoryStore + host (pipeline compartido) -> SQLite aquí
crates/songs/            split de slides, importadores (OpenLyrics, ChordPro, OpenSong, texto)
crates/bible/            parser de referencias, importadores (OSIS/USFM/Zefania), versificación
crates/server/           axum: HTTP + WebSocket, pairing, roles, mDNS, rutas /output/*
packages/protocol/       tipos TS GENERADOS + interfaz HollyApi + mock en memoria
packages/slide-renderer/ SlideView (único renderizador), auto-fit, transiciones
packages/ui/             tokens «Cabina nocturna» + componentes base
```

Dirección de dependencias (nunca al revés):

```
core  <-  storage  <-  server  <-  desktop(src-tauri)
core  <-  songs, bible (puros, sin storage)
protocol(TS) <- slide-renderer, ui(solo tokens) <- apps
```

`core` no conoce Tauri, red ni base de datos. `songs` y `bible` tampoco dependen de `storage`: el
pegamento (expandir un ítem del culto a slides leyendo repos) vive en `desktop`/`server` vía
`hollyshow_storage::host`.

## 2. Tabla de propiedad

| Directorio                | Dueño                   | Contrato que no se rompe sin avisar                                                                                      |
| ------------------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `crates/core`             | Builder A (núcleo)      | `Command`, `Event`, `PresentationState`, `apply`, `protocol.rs`. Cambiar el JSON de cable exige subir `PROTOCOL_VERSION` |
| `crates/storage`          | Builder B (datos)       | Traits en `repo.rs`; `host.rs` (`dispatch`, `execute_query`, `enrich_command`)                                           |
| `crates/songs`            | Builder C (canciones)   | `SlideSplitter`, `SongImporter`                                                                                          |
| `crates/bible`            | Builder D (Biblia)      | `ReferenceParser`, `BibleImporter`; textos solo dominio público                                                          |
| `crates/server`           | Builder E (red)         | `App`, `ServerConfig`, autorización con `Role::can_dispatch/can_query`                                                   |
| `apps/desktop/src-tauri`  | Builder F (shell)       | Nombres de comandos IPC y `hollyshow://update` (espejados en `src/api/tauri.ts`)                                         |
| `apps/desktop/src`        | Builder G (UI operador) | Rutas `/operator`, `/output/:id`, `/stage`                                                                               |
| `apps/remote`             | Builder H (remoto)      | Solo habla `HollyApi`; sin lógica de dominio                                                                             |
| `packages/protocol`       | Arquitecto              | `generated/` NO se edita a mano (`pnpm gen:types`)                                                                       |
| `packages/slide-renderer` | Builder I (render)      | `SlideView` props; `fitFontSize`                                                                                         |
| `packages/ui`             | Builder G               | Tokens CSS (`--hs-*`)                                                                                                    |
| `.github/`, raíz, `docs/` | Arquitecto              |                                                                                                                          |

## 3. Flujo de datos

```
UI (operador / remoto / API)
  │  Command { kind, ... }               (invoke "dispatch"  |  WS ClientMsg::Command)
  ▼
host::dispatch(engine, repos, id, cmd)
  ├─ enrich_command   resuelve lo que el reductor no puede saber (texto bíblico de ShowBible)
  ├─ Engine::dispatch dedupe por CommandId -> apply(state, cmd) -> (state', events)
  └─ Ack { ok, version, error, duplicate }  ← respuesta al emisor
        │ si cambió el estado: broadcast::Sender<Update { version, view, events }>
        ├─► puente Tauri:  app.emit("hollyshow://update") → ventanas operador / salida / escenario
        └─► servidor WS:   ServerMsg::State { view } + ServerMsg::Event a cada remoto (filtrado por Role)
```

Reglas del reductor (`crates/core/src/reducer.rs`, PLAN §3.1/§3.4):

- Un comando rechazado no cambia nada ni sube la versión.
- Los absolutos (`goTo`, `blackout`...) son idempotentes: repetirlos no cambia versión ni emite eventos.
- Los relativos (`next`, `prev`) llevan `expectedVersion`; si está desfasado -> `version_mismatch`.
  Un doble toque o un reintento nunca salta dos slides.
- Cada cambio sube `version` en exactamente 1. El reloj nunca se lee dentro del reductor (`atMs` lo pone el host).
- `Engine` además deduplica por `CommandId` (ventana de 512): un reintento tras perder el ack devuelve el resultado original.
- Mostrar contenido levanta `clear` y `logo`; `blackout` solo se levanta con `blackout{on:false}` (estado de seguridad).

`PresentationState` = `StateView` (barato, se difunde en cada cambio) + `show` (slides expandidos, grande).
Los clientes piden snapshot al conectar y de nuevo solo cuando cambia `showRevision`.
Las salidas guardan una copia del slide en `live`/`preview` (`Cue`), por eso la ventana de salida no
necesita el show y sigue mostrando lo último aunque el operador se cuelgue (PLAN §3.1.5).

## 4. Modelo de hilos

- `Engine` guarda el estado tras un `std::sync::Mutex` con secciones críticas de microsegundos; no hay `await` dentro. Se puede llamar
  desde comandos Tauri (pool de hilos) y tareas tokio por igual.
- Los repositorios son **síncronos** (rusqlite bloquea). Desde código async usar `tokio::task::spawn_blocking`.
  Los comandos Tauri sincrónicos ya corren fuera del hilo principal.
- Difusión: `tokio::sync::broadcast` (capacidad 256). Un consumidor lento recibe `Lagged` y se resincroniza con la vista
  actual (no se reproducen eventos perdidos), así nunca queda atascado.
- El hilo principal de Tauri solo gestiona UI/ventanas; nada pesado ahí.
- El servidor axum corre en el runtime de Tauri (`tauri::async_runtime`); un único `Arc<Engine>` compartido.

## 5. Manejo de errores

- `CoreError` (reductor): errores de dominio con `code()` estable (`version_mismatch`, `unknown_item`, ...). Se serializa como `WireError {code, message}`.
- `StorageError`: `NotFound | Conflict | Invalid | Backend`. Nunca se filtran mensajes de SQLite a un remoto sin pasar por `WireError`.
- Tauri IPC: `dispatch` devuelve siempre `Ack` (ok o error de dominio); `query` devuelve `Err(WireError)`. Fallo de transporte = promesa rechazada.
- Nada de `unwrap()` en rutas de culto. Un pánico no debe tumbar la salida: la ventana de salida es independiente.
- La UI muestra los errores de `Ack` como toasts; `version_mismatch` se ignora silenciosamente tras refrescar la vista.

## 6. Cómo añadir un comando de extremo a extremo

Ejemplo: `Command::Freeze { on }`.

1. `crates/core/src/command.rs`: añadir la variante (con `#[ts(type = "number")]` en `u64`/`i64`).
2. `crates/core/src/state.rs`: campo en `PresentationState` **y** en `StateView`/`view()`.
3. `crates/core/src/event.rs`: variante `Event::FreezeChanged { on }`.
4. `crates/core/src/reducer.rs`: rama en `apply` (absoluta -> idempotente) + tests (idempotencia, rechazo, versión).
5. `crates/core/src/protocol.rs`: ajustar `Role::can_dispatch` (¿quién puede?) y su test.
6. `pnpm gen:types` y commitear `packages/protocol/src/generated`.
7. `packages/protocol/src/commands.ts`: builder `commands.freeze(on)`; si el mock lo necesita, `reducer.ts`.
8. UI: llamar `api.dispatch(commands.freeze(true))`; leer `view.freeze`.
9. Si necesita datos externos (como la Biblia), añadir el caso a `host::enrich_command`.
10. CI comprueba que `generated/` esté al día; `PROTOCOL_VERSION` solo se sube si el cambio rompe clientes existentes.

Consultas nuevas: variante en `QueryRequest`/`QueryResponse` + `host::execute_query` + permiso en `Role::can_query` +
entrada en `QueryResponseKinds` (`packages/protocol/src/api.ts`).

## 7. Adaptadores de `HollyApi`

- `tauri` (hecho, `apps/desktop/src/api/tauri.ts`): `invoke("dispatch" | "query" | "get_snapshot")` + `listen("hollyshow://update")`.
- `mock` (hecho, `packages/protocol`): estado en memoria con un espejo mínimo del reductor, para desarrollo y tests de UI.
- `websocket` (pendiente, builder H, en `apps/remote`): `hello` -> `welcome` -> `snapshot`; heartbeat `ping` cada 5 s;
  reconexión exponencial; al reconectar `resync` si `showRevision` difiere. Reutiliza `ClientMsg`/`ServerMsg` generados.

## 8. Estrategia de pruebas

| Capa                          | Herramienta                                                   | Qué cubre                                                                    |
| ----------------------------- | ------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Reductor / Engine / protocolo | `cargo test -p hollyshow-core`                                | reglas de versión, idempotencia, roles, forma JSON del cable                 |
| Persistencia                  | `cargo test -p hollyshow-storage`                             | contrato de los traits (`MemoryStore` y, luego, SQLite con la misma batería) |
| Servidor                      | `cargo test -p hollyshow-server`                              | rutas HTTP con `tower::oneshot`; WS con cliente tokio-tungstenite            |
| TS generado                   | `pnpm gen:types` + `git diff --exit-code` en CI               | los tipos nunca divergen de Rust                                             |
| Renderer y remoto             | Vitest (`pnpm test`) + Playwright (a añadir)                  | auto-fit, SlideView, adaptadores                                             |
| Escritorio E2E                | Windows/Linux en CI (tauri-driver); checklist manual en macOS | `tauri-driver` no soporta macOS                                              |

Soak test de 4 h y pruebas de red con varios celulares: fuera del CI, checklist manual de la Fase 6.

## 9. Compilación y distribución

- Desarrollo: `pnpm dev` (Tauri + Vite), `pnpm dev:web` (solo navegador con `mock`), `pnpm dev:remote`.
- `pnpm build` compila el remoto y luego `tauri build` con los objetivos de `tauri.conf.json`
  (dmg/app, nsis/msi, deb/appimage; cada SO construye los suyos).
- Release: tag `v*` o _workflow_dispatch_ -> `.github/workflows/release.yml` (macOS universal, Windows, Ubuntu 22.04) -> release en borrador.
- **Sin firma por ahora.** Los secretos `APPLE_*` y de firma de Windows están como bloques comentados en `release.yml`;
  sin ellos macOS requiere "clic derecho -> Abrir" y Windows muestra SmartScreen.

## 10. Fuera de alcance de v1 (solo TODOs documentados)

Voz/subtítulos con IA, NDI, MIDI/OSC, luces, Planning Center, sincronización en la nube, respaldo en caliente
y servidor de auto-actualización (`tauri-plugin-updater` queda para la Fase 6).
