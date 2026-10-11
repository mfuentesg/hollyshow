# HollyShow: arquitectura de implementación

Complementa `docs/PLAN.md` (qué y por qué) con el cómo del código. Fuente de verdad visual: `docs/design/`.
El código manda sobre este documento: si divergen, corrige el documento en el mismo commit.

## 1. Mapa de módulos

```
apps/desktop/            Tauri 2 + React (operador, salida, escenario: una SPA, una ruta por ventana)
  src/                   routes/{Operator,Output,Stage}.tsx, api/tauri.ts (adaptador HollyApi), i18n/
  src-tauri/             wiring: IPC commands, ventanas, puente motor -> webviews (solo depende de app)
apps/remote/             SPA móvil (mobile-first); su dist/ se embebe en crates/server (rust-embed)
crates/core/             dominio PURO: tipos, Command (cable) / EngineCommand (interno), reductor, protocolo
                         de cable, proyección por rol. Solo serde + ts-rs: sin tokio, sin red, sin BD.
crates/engine/           Engine: dueño del estado, reloj del host, dedupe por (sesión, id), broadcast (tokio)
crates/storage-api/      traits de repositorio + error. MemoryStore tras la feature `testing`
crates/storage-sqlite/   implementación SQLite (rusqlite bundled + FTS5) de esos traits
crates/songs/            SlideSplitter (+ DefaultSplitter), importadores (OpenLyrics, ChordPro, OpenSong, texto)
crates/bible/            parser de referencias, importadores, build_bible_slide (composición del slide bíblico)
crates/app/              CASOS DE USO: autorización por rol, enriquecimiento de comandos, playlist -> show,
                         consultas, persistencia de sesión, RoleFeed (proyección + lag). Único punto de entrada.
crates/server/           axum: HTTP + WebSocket, pairing, mDNS, remoto embebido. Solo depende de app
packages/protocol/       tipos TS GENERADOS + interfaz HollyApi + mock en memoria
packages/slide-renderer/ SlideView (único renderizador), auto-fit, transiciones
packages/ui/             tokens «Cabina nocturna» + componentes base
```

Dirección de dependencias (nunca al revés):

```
core  <-  engine  <-  app  <-  server  <-  desktop(src-tauri)
core  <-  storage-api  <-  app            storage-api  <-  storage-sqlite  (lo inyecta desktop)
core  <-  songs, bible  <-  app           (puros, sin storage ni engine)
protocol(TS) <- slide-renderer, ui(solo tokens) <- apps
```

`core` no conoce Tauri, red, base de datos ni runtime async. `server` y `desktop` no hablan nunca con `engine` ni con los
repositorios directamente: todo pasa por `hollyshow_app::AppService`, de modo que Tauri y WebSocket se comportan igual.
Las escrituras de biblioteca (canciones, listas, temas, Biblias, medios) pasan por `AppService::save_*`/`delete_*` o por
`AppService::write_library`, que (a) rechaza con `locked` en modo culto, (b) ejecuta la escritura bajo la misma compuerta que
serializa el comando `ServiceMode` (sin carrera) y (c) llama a `library_changed`/`playlist_saved`. `SetActivePlaylist` y
`RefreshShow` también se rechazan en modo culto. El shell no puede olvidar ninguno de los pasos.

## 2. Tabla de propiedad

| Directorio                | Dueño                   | Contrato que no se rompe sin avisar                                                                                          |
| ------------------------- | ----------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `crates/core`             | Builder A (núcleo)      | `Command` (cable), `EngineCommand`, `Event`, `apply`, `protocol.rs`. Un cambio incompatible del JSON sube `PROTOCOL_VERSION` |
| `crates/engine`           | Builder A (núcleo)      | `Engine::dispatch(session, id, cmd)`, `Clock`                                                                                |
| `crates/storage-api`      | Builder B (datos)       | Traits en `repo.rs`                                                                                                          |
| `crates/storage-sqlite`   | Builder B (datos)       | Implementa los traits; migraciones, FTS5, copias                                                                             |
| `crates/songs`            | Builder C (canciones)   | `SlideSplitter`, `SplitOptions`, `SongImporter`                                                                              |
| `crates/bible`            | Builder D (Biblia)      | `ReferenceParser`, `BibleImporter`, `build_bible_slide`; textos solo dominio público                                         |
| `crates/app`              | Arquitecto              | `AppService` (`dispatch`, `query`, `feed`, `snapshot_msg`, `restore`, `library_changed`), `ClientCtx`                        |
| `crates/server`           | Builder E (red)         | `App`, `ServerConfig`; usa solo `AppService`                                                                                 |
| `apps/desktop/src-tauri`  | Builder F (shell)       | Nombres de comandos IPC y `hollyshow://update` (espejados en `src/api/tauri.ts`)                                             |
| `apps/desktop/src`        | Builder G (UI operador) | Rutas `/operator`, `/output/:id`, `/stage`                                                                                   |
| `apps/remote`             | Builder H (remoto)      | Solo habla `HollyApi`; sin lógica de dominio                                                                                 |
| `packages/protocol`       | Arquitecto              | `generated/` NO se edita a mano (`pnpm gen:types`)                                                                           |
| `packages/slide-renderer` | Builder I (render)      | `SlideView` props; `fitFontSize`                                                                                             |
| `packages/ui`             | Builder G               | Tokens CSS (`--hs-*`)                                                                                                        |
| `.github/`, raíz, `docs/` | Arquitecto              |                                                                                                                              |

**Tipos exportados a TypeScript:** solo `hollyshow-core` define tipos con `#[ts(export)]`. El resto de crates los
referencian. `pnpm gen:types` igualmente ejecuta los tests de exportación de todo el workspace para que un tipo exportado en
otro sitio salga en el diff de CI en vez de perderse en silencio.

## 3. Flujo de datos

```
UI (operador / remoto / API)
  │  Command { kind, ... }               (invoke "dispatch"  |  WS ClientMsg::Command)
  ▼
AppService::dispatch(ClientCtx{session, role}, id, cmd)
  ├─ Role::can_dispatch      autoriza (Pastor solo ShowBible; Unknown nadie)
  ├─ enriquece               ShowBible -> slide con texto de la biblioteca (el cliente nunca aporta el slide)
  │                          SetActivePlaylist / RefreshShow -> playlist -> show (songs + bible) -> LoadShow
  ├─ Engine::dispatch(session, id, EngineCommand)   reloj del host -> apply(state, ctx, cmd) -> (state', events)
  │                          dedupe por (SessionId, CommandId), también los errores
  ├─ persiste (mejor esfuerzo) playlist activa, live, timers, modo culto -> settings "session.v1"
  └─ Ack { ok, version, error, duplicate }   ← respuesta al emisor (igual en IPC y en WS: ServerMsg::Ack { id, ack })
        │ si cambió el estado: broadcast::Sender<Update { version, view, events }>   (sin filtrar, uso del host)
        ├─► puente Tauri:  app.emit("hollyshow://update") → ventanas operador / salida / escenario (de confianza)
        └─► RoleFeed(role): ServerMsg::from_update -> State + Events FILTRADOS por rol; si el cliente se retrasa
                            (Lagged): State fresco + Resync{lagged}; el cliente pide Snapshot
```

Reglas del reductor (`crates/core/src/reducer.rs`, PLAN §3.1/§3.4):

- Un comando rechazado no cambia nada ni sube la versión.
- Los absolutos (`goTo`, `blackout`...) son idempotentes: repetirlos no cambia versión ni emite eventos.
- Los relativos (`next`, `prev`) llevan `expectedVersion`; si está desfasado -> `version_mismatch`.
  Un doble toque o un reintento nunca salta dos slides.
- Cada cambio sube `version` en exactamente 1. El reloj nunca se lee dentro del reductor: el `Engine` estampa `Ctx.now_ms`
  desde su `Clock`. **Ningún cliente envía horas** (`StartTimer`/`PauseTimer` no llevan `atMs`; `SetTimer` ignora el estado
  de ejecución que traiga el cliente).
- El reductor solo consume `EngineCommand` (no serializable). Un `Command` de cable `ShowBible`, `SetActivePlaylist`,
  `RefreshShow` o `Unknown` llega ahí rechazado: solo el host puede construir `EngineCommand::{ShowBible, LoadShow, Restore}`.
- Mostrar contenido levanta `clear` y `logo`; `blackout` solo se levanta con `blackout{on:false}` (estado de seguridad).

`PresentationState` = `StateView` (barato, se difunde en cada cambio) + `show` (slides expandidos, grande).
Los clientes piden snapshot al conectar y de nuevo solo cuando cambia `showRevision`.
Las salidas guardan una copia del slide en `live`/`preview` (`Cue`), por eso la ventana de salida no
necesita el show y sigue mostrando lo último aunque el operador se cuelgue (PLAN §3.1.5).

### Propiedad del show

El estado Rust es la fuente de verdad **del show expandido**, pero la fuente del **contenido** es la biblioteca:

- `SetActivePlaylist(id)` y `RefreshShow` (solo Operador) expanden `Playlist -> Vec<ShowEntry>` (`app::build_show`) y cargan el show.
- `AppService::library_changed(scope)` (canciones, Biblia, temas) y `playlist_saved(id)` reconstruyen el show si afecta a
  la playlist activa y publican el evento para que los clientes refresquen sus consultas. Quien guarda una canción/playlist
  **debe** llamar a estos métodos.
- **Edición en vivo (`UpdateSlide`) es efímera**: se superpone al show del servicio actual (se reaplica tras cada
  reconstrucción, anclada al id del slide) y se descarta al abrir otra playlist. No escribe en la canción; guardar
  el cambio en la biblioteca será una acción explícita aparte.
- **Persistencia de sesión**: tras cada comando aceptado, `AppService` guarda (mejor esfuerzo) playlist activa, cue en vivo,
  timers y modo culto en `settings["session.v1"]`; `restore()` los recupera al arrancar (reabre la playlist y restaura
  con `EngineCommand::Restore`). Un JSON corrupto se ignora con un aviso.

### Proyección por rol y protocolo

- `project(state, role) -> RoleSnapshot` y `project_view(view, role)` son la **única** vía por la que el estado llega a la red:
  Congregación no recibe preview, mensajes de escenario, timers, salidas ni el show; Escenario recibe preview (es su «siguiente»)
  pero no el show. `Update::for_role` y `Event::visible_to` filtran eventos igual. `ServerMsg::from_update` los usa.
- `Hello` lleva `protocolMin/protocolMax` y `capabilities`; `negotiate()` elige la mayor revisión común y `Welcome` devuelve la
  negociada, las capacidades y `nowMs` (el reloj del servidor, para que los cronómetros de los remotos coincidan con el proyector;
  `State` y `Snapshot` también llevan `nowMs`).
- Evolución: `Command`, `Event`, `ClientMsg`, `ServerMsg`, `QueryRequest` y `QueryResponse` tienen una variante `Unknown`
  (`#[serde(other)]`): un par antiguo degrada (ignora el evento; responde `unsupported` al comando) en vez de perder la conexión.
  `PROTOCOL_MIN..=PROTOCOL_VERSION` se amplía al añadir variantes compatibles; solo un cambio incompatible sube el mínimo.
- Huecos: cada cambio sube la versión en 1 y cada `Event` lleva la versión resultante; un cliente que vea un salto pide `Resync`.
- Ack unificado: `ServerMsg::Ack { id, ack: Ack }` incrusta el mismo `Ack` del IPC. Las consultas usan `QueryId`.

## 4. Modelo de hilos

- `Engine` guarda el estado tras un `std::sync::Mutex` con secciones críticas de microsegundos; no hay `await` dentro. El reductor
  trabaja sobre una copia y solo se asigna si tiene éxito, así que un pánico no deja el estado a medias y un lock envenenado
  se puede recuperar sin riesgo.
- Los repositorios son **síncronos** (rusqlite bloquea). Desde código async usar `tokio::task::spawn_blocking`.
  Los comandos Tauri **sin** `async` se ejecutan en el hilo principal (el que atiende las ventanas de proyección): todos los
  comandos del shell son `#[tauri::command(async)]` o `async fn` (con `spawn_blocking` para lo pesado, como instalar una Biblia).
- Difusión: `tokio::sync::broadcast` (capacidad 256). Un consumidor lento recibe `Lagged`; `RoleFeed` lo convierte en
  `State` fresco + `Resync` (no se reproducen eventos perdidos), así nunca atasca al motor ni a los demás.
- **Los pánicos no abortan la app.** El perfil release usa `unwind` (sin `panic = "abort"`). `AppService::dispatch/query`
  contienen los pánicos (`catch_unwind`) y responden `internal`; cada conexión WS corre en su propia tarea tokio (un pánico
  la tumba a ella sola); `install_panic_hook` registra cada pánico. Un móvil que envíe un frame malformado jamás debe
  dejar la pantalla en negro.
- El hilo principal de Tauri solo gestiona UI/ventanas; nada pesado ahí.
- El servidor axum corre en **su propio runtime tokio** (2 hilos, `Remote` en `apps/desktop/src-tauri/src/remote.rs`), no en
  `tauri::async_runtime`: el trabajo bloqueante de los comandos IPC (SQLite, copias, instalar Biblias) nunca retrasa accepts,
  latidos ni acks de los móviles. Un único `Arc<AppService>` compartido. Las `async fn` del shell no llaman a los
  repositorios ni a `library_changed` en línea: usan `state::blocking`, que va a `spawn_blocking` con un semáforo (4 a la vez).
- **ACL de Tauri**: `build.rs` declara todos los comandos (`AppManifest`), de modo que cada uno tiene un permiso `allow-<nombre>`.
  `capabilities/default.json` (ventana `main`) los concede todos; `capabilities/render-only.json` (`output-*`, `stage`) solo
  `get_snapshot`, `query` y `report_output_stats`. Un test comprueba que el manifiesto coincide con `generate_handler!`.
- Registro de uso (informe CCLI): al pasar una canción a vivo, `AppService` la encola (una vez por servicio) en un hilo
  (`usage.rs`) que escribe en `usage_log`. Los dispositivos emparejados viven solo en `settings['server.devices.v1']`.

## 5. Manejo de errores

- `CoreError` (reductor): errores de dominio con `code()` estable (`version_mismatch`, `unknown_item`, ...). Se serializa como `WireError {code, message}`.
- `StorageError`: `NotFound | Conflict | Invalid | Backend`. Nunca se filtran mensajes de SQLite a un remoto sin pasar por `WireError`.
- Tauri IPC: `dispatch` devuelve siempre `Ack` (ok o error de dominio); `query` devuelve `Err(WireError)`. Fallo de transporte = promesa rechazada.
- Nada de `unwrap()` en rutas de culto. Un pánico no debe tumbar la salida: se contiene (ver §4) y se responde `internal`.
- Códigos estables añadidos: `unsupported` (comando/consulta de una revisión más nueva o sin sentido en el reductor), `internal`, `forbidden`, `protocol_mismatch`.
- La UI muestra los errores de `Ack` como toasts; `version_mismatch` se ignora silenciosamente tras refrescar la vista.

## 6. Cómo añadir un comando de extremo a extremo

Ejemplo: `Command::Freeze { on }`.

1. `crates/core/src/command.rs`: añadir la variante a `Command` (con `#[ts(type = "number")]` en `u64`/`i64`). Nunca campos que
   resuelva el host ni horas: eso va en `EngineCommand`/`Ctx`.
2. `crates/core/src/state.rs`: campo en `PresentationState` **y** en `StateView`/`view()`.
3. `crates/core/src/event.rs`: variante `Event::FreezeChanged { on }`.
4. `crates/core/src/reducer.rs`: rama en `apply` (absoluta -> idempotente) + tests (idempotencia, rechazo, versión).
5. `crates/core/src/protocol.rs`: ajustar `Role::can_dispatch` (¿quién puede?), `Event::visible_to` y `project_view` si lo ven los remotos; con test.
6. `pnpm gen:types` y commitear `packages/protocol/src/generated`.
7. `packages/protocol/src/commands.ts`: builder `commands.freeze(on)`; si el mock lo necesita, `reducer.ts`.
8. UI: llamar `api.dispatch(commands.freeze(true))`; leer `view.freeze`.
9. Si necesita datos externos (como la Biblia), resolverlos en `AppService::dispatch_inner` y pasar un `EngineCommand` propio.
10. CI comprueba que `generated/` esté al día (incluye archivos nuevos sin commitear); `PROTOCOL_VERSION` solo se sube si el cambio rompe clientes
    existentes (añadir variantes no lo rompe gracias a `Unknown`).

Consultas nuevas: variante en `QueryRequest`/`QueryResponse` + `app::query::execute` + permiso en `Role::can_query` +
entrada en `QueryResponseKinds` (`packages/protocol/src/api.ts`).

## 7. Adaptadores de `HollyApi`

- `tauri` (hecho, `apps/desktop/src/api/tauri.ts`): `invoke("dispatch" | "query" | "get_snapshot")` + `listen("hollyshow://update")`.
- `mock` (hecho, `packages/protocol`): estado en memoria con un espejo mínimo del reductor, para desarrollo y tests de UI.
- `websocket` (pendiente, builder H, en `apps/remote`): `hello` -> `welcome` -> `snapshot`; heartbeat `ping` cada 5 s;
  reconexión exponencial; al reconectar `resync` si `showRevision` difiere. Reutiliza `ClientMsg`/`ServerMsg` generados.

## 8. Estrategia de pruebas

| Capa                 | Herramienta                                                   | Qué cubre                                                                  |
| -------------------- | ------------------------------------------------------------- | -------------------------------------------------------------------------- |
| Reductor y protocolo | `cargo test -p hollyshow-core`                                | reglas de versión, idempotencia, roles, proyección, forma JSON, `Unknown`  |
| Engine               | `cargo test -p hollyshow-engine`                              | dedupe por sesión, errores cacheados, reloj del host, lag                  |
| Casos de uso         | `cargo test -p hollyshow-app`                                 | autorización, show, edición en vivo, restauración, pánicos, `RoleFeed`     |
| Persistencia         | `cargo test -p hollyshow-storage-sqlite`                      | contrato de los traits (misma batería que `MemoryStore`)                   |
| Servidor             | `cargo test -p hollyshow-server`                              | rutas HTTP con `tower::oneshot`; WS con cliente tokio-tungstenite          |
| TS generado          | `pnpm gen:types` + `git status --porcelain` en CI             | los tipos nunca divergen de Rust (ni quedan archivos nuevos sin commitear) |
| Renderer y remoto    | Vitest (`pnpm test`) + Playwright (a añadir)                  | auto-fit, SlideView, adaptadores                                           |
| Escritorio E2E       | Windows/Linux en CI (tauri-driver); checklist manual en macOS | `tauri-driver` no soporta macOS                                            |

Soak test de 4 h y pruebas de red con varios celulares: fuera del CI, checklist manual de la Fase 6.

## 9. Compilación y distribución

- **Clon nuevo:** `pnpm bootstrap` (instala y construye los frontends). Los crates `server` y `desktop` incrustan `dist/` en
  compilación; sus `build.rs` generan una página provisional cuando falta, para que `cargo check/test/clippy` funcionen sin
  haber construido nada, y **rechazan** un build `release` sin los frontends reales.
- Desarrollo: `pnpm dev` (Tauri + Vite), `pnpm dev:web` (solo navegador con `mock`), `pnpm dev:remote`.
- `pnpm build` compila el remoto y luego `tauri build` con los objetivos de `tauri.conf.json`
  (dmg/app, nsis/msi, deb/appimage; cada SO construye los suyos).
- **Versión única:** `Cargo.toml [workspace.package]`; `package.json` (raíz, desktop, remote) debe coincidir y `tauri.conf.json`
  no lleva `version` (Tauri la lee de Cargo). `scripts/check-version.mjs [tag]` lo verifica en CI y en el release.
- Release: tag `vX.Y.Z` (prerelease solo numérico, `vX.Y.Z-N`, porque WiX/msi rechaza `-rc.1`) o _workflow_dispatch_ ->
  `release.yml`: `version` (tag = versión) -> `verify` (reutiliza `ci.yml`: fmt, clippy, tests, tipos generados, lint) ->
  `build` (macOS universal, Windows, Ubuntu 22.04) -> release en borrador. `tauriScript: pnpm tauri` explícito, caché de
  Rust por plataforma, `timeout-minutes`, artefactos subidos también si falla una pata.
- **Sin firma por ahora.** Los secretos `APPLE_*` y de firma de Windows están como bloques comentados en `release.yml`;
  sin ellos macOS requiere "clic derecho -> Abrir" y Windows muestra SmartScreen.

## 10. Fuera de alcance de v1 (solo TODOs documentados)

Voz/subtítulos con IA, NDI, MIDI/OSC, luces, Planning Center, sincronización en la nube, respaldo en caliente
y servidor de auto-actualización propio (el cliente de actualización ya está: `src-tauri/src/updater.rs` contra GitHub Releases).

## 11. Decisiones (revisión de arquitectura del andamiaje)

Resultado de la revisión independiente del andamiaje. Cada hallazgo se corrigió; aquí queda el porqué.

1. **`Command` de cable vs `EngineCommand` interno.** `ShowBible` ya no lleva `resolved`; `LoadShow` salió del cable;
   `UpdateSlide` se queda en el cable pero solo para Operador (el rol Pastor solo puede `ShowBible`) y el host fija el id del
   slide. El reductor nunca deserializa de la red. Si un cliente envía un campo `resolved`, serde lo descarta (test).
2. **`core` puro.** `Engine` y tokio pasan a `hollyshow-engine`. `core` solo tiene serde y ts-rs (+ unicode-normalization).
3. **Capa de aplicación (`hollyshow-app`)** en lugar de `storage::host`. `storage` se parte en `storage-api` (traits, sin BD) y
   `storage-sqlite` (rusqlite); `MemoryStore` queda tras la feature `testing` (solo tests; el escritorio usa SQLite).
   La composición del slide bíblico vive en `bible`, el split de canciones en `songs` (`DefaultSplitter`).
4. **Idempotencia por `(SessionId, CommandId)`**, ventana de 128 por sesión, máximo 256 sesiones (se expulsa la menos usada),
   y los errores también se cachean. `SessionId` lo asigna el servidor al aceptar el `Hello`.
5. **Los relojes son del host.** Sin `atMs` en el cable; `Ctx.now_ms` sale del `Clock` del engine; `Welcome/State/Snapshot`
   llevan `nowMs` para corregir el desfase en los clientes.
6. **`panic = "abort"` eliminado.** Ver §4. El coste en tamaño de binario es irrelevante frente a proyectar en negro en pleno culto.
7. **Proyección por rol obligatoria** (`project`, `project_view`, `Event::visible_to`) y contrato de lag (`RoleFeed`, `Resync`),
   con tests. La difusión `Update` sin filtrar es solo para las ventanas de Tauri (código propio, de confianza); el servidor
   usa siempre `ServerMsg::from_update`.
8. **Evolución del protocolo:** rango `protocolMin/Max`, `capabilities`, variantes `Unknown`, `Ack` unificado, `QueryId`.
   `PROTOCOL_VERSION` sigue en 1 porque aún no hay clientes publicados; desde el primer release cualquier cambio incompatible
   debe subirla.
9. **Show con autoridad:** ver «Propiedad del show» (§3).
10. **Release con compuerta de calidad y coherencia de versión** (§9). Se eligió quitar `version` de `tauri.conf.json` en vez de
    generarla en tres sitios.
11. **Embebido del remoto sin `dist/` previo:** `build.rs` en `server` (copia a `OUT_DIR`, o página provisional) y en `desktop`.
    Se descartó `#[folder]` directo a `apps/remote/dist` porque rust-embed falla en release si falta y ensucia el árbol.
12. **Tipos TS:** `cargo test` plano escribe en `target/ts-bindings` (ya no toca archivos versionados); `pnpm gen:types`
    genera en un directorio temporal, ejecuta los tests de exportación de todo el workspace y hace el intercambio al final;
    CI usa `git status --porcelain` para detectar también archivos nuevos. El barril `index.ts` lo genera el script.
    Los conflictos de fusión en `generated/` se resuelven regenerando (`pnpm gen:types`), nunca a mano.

### Deuda conocida (documentada, no bloqueante)

- Cada pata de WebSocket debe llamar a `AppService::feed` **antes** de `snapshot_msg` (ver `RoleFeed`).
- La persistencia de sesión escribe de forma síncrona tras cada comando aceptado (microsegundos con SQLite WAL); si midiéramos
  latencia visible, se moverá a un suscriptor del broadcast.

## 12. Núcleo: añadidos de v1 (`crates/core`)

- **Salidas con capas propias:** `output_layers[output]` (`OutputLayers`: fondo, media, texto, overlay, blackout) junto a
  `outputs[output]` (`OutputLook`). `StateView::frame_for(output)` combina flags globales + look + capas en un `OutputFrame`
  (el escenario con `follows_blackout=false` ignora el blackout global; el blackout por salida solo afecta a esa salida).
  Comandos: `SetOutputLayers`, `RemoveOutput` (`main` no se puede borrar).
- **Reloj por ítem:** `item_clock` arranca (reloj del host) al cambiar el ítem en vivo si tiene `duration_secs`;
  `ItemClock::{remaining,overtime}_ms_at(now)`. `ResetItemClock` lo reinicia. `Timer::overtime_ms_at` para cuentas atrás.
- **Modo culto:** con `service_mode` el reductor rechaza (`locked`) `UpdateSlide`, `SetOutputLook` y `RemoveOutput`;
  navegar, blackout/clear/logo, capas, mensajes y timers siguen permitidos. `ServiceMode{on:false}` siempre pasa.
- **Límites:** mensajes/overlays ≤ 280 caracteres, cuentas atrás ≤ 24 h, ids ≤ 64 (`invalid`).
- **Playlist (puro):** `Playlist::{add_item, move_item, remove_item, duplicate_item, to_template, instantiate, schedule}`;
  la persistencia sigue siendo del caso de uso en `app`.
- **Troceado (puro, `core::split`):** `split_song` (orden del arreglo, `---`/líneas en blanco, `max_lines` equilibrado,
  `size_group` compartido) y `split_passage` (1..=4 versiones cortadas en los mismos versículos; hueco = «—»).
  `songs`/`bible` pueden delegar en ellos. `Slide.size_group` pide un tamaño de letra común a las diapositivas del grupo.
- Al mostrar contenido que levanta `clear`/`logo` ahora se emiten `ClearChanged/LogoChanged{on:false}`.

## 13. El shell de escritorio (`apps/desktop/src-tauri`)

Raíz de composición: nada de reglas de negocio, solo cableado. `AppState` (clonable, todo en `Arc`) se crea en `setup` y se
registra con `app.manage`.

| Módulo        | Responsabilidad                                                                                                                                                                                                                                                                                                                           |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `state.rs`    | `AppState::open`: SQLite en el directorio de datos del SO (`app_data_dir`), copia diaria (14), recuperación de una BD corrupta (copia más reciente o biblioteca vacía; la rota se conserva), contenido inicial (tema y plantilla, una sola vez), `session.running` para detectar cierres bruscos, `StartupInfo` («retomar donde estabas») |
| `commands.rs` | `dispatch` / `query` / `get_snapshot` (los mismos casos de uso que el servidor LAN, como operador local)                                                                                                                                                                                                                                  |
| `host.rs`     | Ajustes, escrituras de biblioteca (canciones, listas, temas; importación de canciones), remoto, salud. **Todas las escrituras van por `AppService` (`write_library`), que las rechaza con `locked` en modo culto** y avisa con `library_changed / playlist_saved`                                                                                                          |
| `bibles.rs`   | Instalación en segundo plano de las Biblias incluidas (primer arranque), catálogo, instalar/importar/quitar/ordenar versiones                                                                                                                                                                                                             |
| `remote.rs`   | Ciclo de vida del servidor LAN (arranca al abrir; si el puerto 7420 está ocupado toma uno libre), PIN/QR con el rol configurado, dispositivos                                                                                                                                                                                             |
| `windows.rs`  | Ventanas de salida/escenario: sin bordes, tamaño del monitor, **nunca** pantalla completa nativa; elección persistida; vigilancia de monitores (ver abajo); cierre bloqueado en modo culto                                                                                                                                                |
| `media.rs`    | Comandos `media_*` y el protocolo `hollyshow-media://localhost/<id>` que sirve las imágenes descargadas a las salidas (con CORS para la ruta WebGL)                                                                                                                                                                                       |
| `recovery.rs` | Autosave cada 5 s, copia horaria, volcado de la sesión desde el _panic hook_ (en otro hilo con tope de 1,5 s para no bloquearse si el pánico ocurre con el lock del motor tomado)                                                                                                                                                         |
| `power.rs`    | Evita la suspensión mientras el modo culto está activo (`keepawake`: IOKit / `SetThreadExecutionState` / D-Bus); si la plataforma lo rechaza solo se registra                                                                                                                                                                             |
| `health.rs`   | CPU/RAM del proceso (`sysinfo`) y fps medidos por la ventana de salida (`report_output_stats`)                                                                                                                                                                                                                                            |
| `updater.rs`  | Actualización estilo VS Code (comprobar a los 30 s y cada 6 h, descargar en segundo plano, instalar al salir si hay culto o salida en vivo)                                                                                                                                                                                               |

**Monitores (hot-plug).** Tauri no emite evento al conectar o desconectar pantallas: `watch_monitors` consulta cada 1,5 s. Si el
monitor de una ventana desaparece, **primero** se pone la salida en negro (`SetOutputLayers { blackout: true }`, que el modo culto
no bloquea) y luego se oculta la ventana; cuando vuelve una pantalla con el mismo nombre y resolución, la ventana se recoloca
allí. El negro **no** se levanta solo: el operador lo suelta, para que un cable que parpadea no muestre una diapositiva a medias.
Con una sola pantalla las salidas se abren como ventana pequeña, nunca encima del operador.

**Modo culto.** Además del bloqueo del reductor (`Locked`): bloquea las escrituras de biblioteca del shell, impide cerrar la
ventana del operador (evento `hollyshow://close-blocked`), mantiene el equipo despierto y difiere las actualizaciones.

**Recuperación.** `session.v1` (playlist abierta, lo que estaba en pantalla, cronómetros, modo culto) se restaura en `AppService::restore`.
`StartupInfo` le dice a la UI si se retomó una sesión, si el cierre anterior fue brusco y si hubo que restaurar la biblioteca.
Los ajustes `session.*`, `server.*` y `windows.*` son del anfitrión: el webview no puede leerlos ni escribirlos.

**Empaquetado.** `pnpm tauri build` (Linux: `.deb` y `.AppImage`) ejecuta antes `pnpm --filter remote build` para que el servidor
incruste la app móvil. Las Biblias van como recursos (`resources/bibles/*.hsb.gz`); en desarrollo se leen del árbol de fuentes.
