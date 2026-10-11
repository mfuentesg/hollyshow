# HollyShow — Plan de implementación

> Software de presentación para iglesias (letras de canciones, versículos bíblicos, medios), inspirado en Holyrics, con **control remoto por red local** como funcionalidad central.
> Aplicación de escritorio para **macOS y Windows**, construida con **Tauri 2 (Rust)** + **React**.

---

## 1. Visión y diferenciadores

| Holyrics hoy | HollyShow (objetivo) |
|---|---|
| UI densa, curva de aprendizaje alta | UI moderna, flujo de "culto" (playlist) claro, atajos de teclado |
| Control remoto limitado / app aparte | **Control remoto web** desde cualquier celular/tablet en la misma red, sin instalar nada (escanear QR) |
| Transiciones básicas | Transiciones CSS + **shaders WebGL**, animación de texto por línea, capas independientes (fondo / texto / overlay) |
| Salida para streaming vía configuraciones extra | **URL de "browser source" para OBS/vMix** con fondo transparente (lower thirds) |
| Java/escritorio pesado | Binario nativo liviano (~10–20 MB), arranque rápido |

**Principio rector:** el día del culto *nada puede fallar*. Fiabilidad > features. Todo funciona offline; la red sólo se usa para el control remoto en LAN.

---

## 2. Features

### 2.1 MVP (imprescindible)

**Canciones**
- Biblioteca con búsqueda instantánea (título, letra, autor), insensible a acentos y mayúsculas ("senor" encuentra "Señor").
- Editor de canción por secciones: Verso 1, Coro, Pre-coro, Puente, Final, etc.
- **Orden de arreglo** (ej. V1 C V2 C P C C) independiente de las secciones escritas.
- División automática en slides (por líneas máximas o por separador manual `---`).
- Metadatos: autor, copyright, número CCLI, tono, BPM, etiquetas.
- Importación: texto plano, **OpenLyrics (XML)**, **ChordPro**, OpenSong. (Investigar exportación de Holyrics, ver §8.)

**Biblia (multiversión)**
- **Múltiples versiones instaladas a la vez**, en varios idiomas (español, inglés, portugués…). Ver §2.4.
- Búsqueda por referencia en español: `Jn 3:16`, `juan 3 16-18`, `1 Co 13`, `Sal 23`, `Ap 21:1-4`. La referencia es independiente de la versión: se escribe una vez y se puede mostrar en cualquiera.
- **Cambiar de versión en vivo** (atajo de teclado / selector en el remoto) manteniendo el mismo pasaje en pantalla.
- **Hasta 2–4 versiones simultáneas** en el mismo slide (lado a lado o apiladas), ej. RVR1909 + KJV para culto bilingüe.
- Búsqueda por texto (FTS) en una versión o en todas.
- Presentación de un versículo o rango, con división automática si no cabe en un slide (sincronizada entre versiones en modo paralelo).
- Historial de versículos mostrados.

**Presentación / salida**
- Ventana de salida a pantalla completa en el monitor elegido (proyector/TV).
- **Preview** (lo próximo) y **Live** (lo que se ve) en la ventana del operador.
- Botones de pánico: **Negro**, **Limpiar texto** (deja fondo), **Logo**.
- Temas: fondo (color, degradado, imagen, video en loop), tipografía, tamaño auto-ajustable, sombra/contorno, márgenes seguros, alineación.
- Transiciones básicas: corte, fundido, crossfade, deslizar.

**Playlist / culto**
- Lista ordenada del culto: canciones, pasajes, imágenes, videos, anuncios.
- Navegación con teclado (flechas, espacio, números para saltar a sección: `C` = coro).
- Guardado automático.

**Control remoto (LAN)**
- Servidor embebido en la app; el celular abre una URL (QR en pantalla).
- Siguiente / anterior, lista de slides del ítem actual (toque para ir), playlist, negro/limpiar.
- Búsqueda rápida de versículo y canción desde el celular, "enviar a preview" o "mostrar ya".
- Emparejamiento con PIN; varios remotos simultáneos sincronizados.

### 2.2 Versión 1.x (alto valor)

- **Stage display** (monitor de escenario / confidence monitor): slide actual, siguiente, reloj, temporizador, mensajes al escenario, acordes.
- **Acordes y transposición** (para músicos, en stage display y en el remoto modo "músico").
- **Browser source para OBS/vMix**: `http://<ip>:<puerto>/output/lower-third` con fondo transparente.
- Efectos avanzados: shaders WebGL (gl-transitions), Ken Burns en fondos, revelado línea por línea, desenfoque de fondo.
- Contadores regresivos (antes del culto), reloj, anuncios en rotación.
- Mensajes / alertas superpuestas (ej. "Padres del niño X, acercarse a recepción").
- Canciones bilingües (traducción en una segunda línea con estilo distinto).
- Roles en el remoto: **Operador**, **Músico** (sólo lectura + acordes), **Pastor** (sólo ver y pedir versículo), **Escenario**.
- Registro de uso de canciones para reportes CCLI.

### 2.3 Futuro / opcional

- Salida **NDI** (requiere SDK de NDI y su licencia — evaluar).
- Sincronización en la nube de la biblioteca entre varias computadoras (opcional, ej. Supabase), siempre offline-first.
- App móvil nativa (sólo si el remoto web queda corto).
- Integración MIDI / Stream Deck / pedaleras Bluetooth (atajos de teclado ya cubren pedaleras HID).
- Presentaciones tipo diapositivas libres (texto + imágenes posicionadas).
- Plugins / API pública (HTTP/WebSocket) para integraciones de terceros.

### 2.4 Biblia: diseño de múltiples versiones

**Gestor de versiones**
- Pantalla "Versiones de la Biblia": lista de instaladas, activar/desactivar, ordenar favoritas, versión **por defecto** (global) y **secundaria** (para modo paralelo).
- Instalar desde: (a) versiones de dominio público incluidas en el instalador, (b) **importar archivo** (OSIS, USFM, Zefania XML, OpenSong XML, y formatos tabulares CSV/JSON), (c) futuro: catálogo descargable de versiones con licencia libre.
- Cada versión guarda: código (`RVR1909`), nombre, idioma, licencia/copyright, si es de dominio público, esquema de versificación, lista de libros incluidos.
- Mostrar el **copyright/abreviatura de la versión** en el slide (configurable por tema), requisito de muchas licencias.

**Referencias independientes de la versión**
- Internamente un pasaje es `(libro canónico OSIS, capítulo, versículo inicio–fin)`, por ejemplo `John.3.16-John.3.18`. El nombre del libro ("Juan", "John", "João") y las abreviaturas se resuelven por idioma, así se puede buscar en español y mostrar en inglés.
- La playlist guarda la referencia canónica, no el texto: al cambiar la versión por defecto, el culto entero cambia sin reeditarlo.

**Diferencias entre versiones (lo complicado)**
- **Versificación distinta:** algunas versiones numeran diferente (títulos de Salmos como v. 1, Malaquías 3–4, Joel 2–3, versículos omitidos como Hch 8:37 en versiones críticas). Usar tablas de mapeo de versificación (como las de SWORD/CrossWire o la versificación estándar "KJV/Vulgata") para alinear versículos al mostrar en paralelo; si un versículo no existe en una versión, mostrar indicador en vez de romper.
- **Canon distinto:** versiones católicas incluyen deuterocanónicos (Tobías, Sabiduría, etc.). El modelo soporta todos los libros; la UI oculta los que la versión no tiene.
- **Marcas en el texto:** palabras en cursiva añadidas por el traductor, palabras de Jesús en rojo, notas al pie, títulos de sección. Importar y guardar marcado mínimo (cursiva, rojo) y permitir activarlo/desactivarlo en el tema; descartar notas al pie para proyección.

**Modo paralelo en pantalla**
- Diseños: columnas lado a lado, arriba/abajo, o versión principal grande + secundaria más pequeña (estilo subtítulo).
- Auto-fit conjunto: las versiones de un mismo slide se dividen en los mismos cortes de versículo para que siempre se vea el mismo pasaje.
- Cada versión puede tener estilo propio (color/fuente) dentro del tema.

**Control remoto**
- Selector de versión en el remoto; el rol "Pastor" puede tener su versión preferida para leer en su dispositivo, independiente de la proyectada.
- Vista "lector": el remoto muestra el pasaje completo en la versión elegida para quien predica, con el versículo en vivo resaltado.

### 2.5 Features modernas (investigación de mercado)

Referencia del mercado actual: Holyrics (gratis, con app remota de pago, plan online con sincronización, API y encuestas por Telegram), ProPresenter (estándar profesional, "reflow" de edición, integración CCLI), FreeShow (open source, multiplataforma, pero con reportes de inestabilidad) y una nueva ola de herramientas con IA (Pewbeam, Loghema, LiveVerse, SmartVerses) que **detectan versículos en la predicación por voz y los proyectan**. Lo que sigue son las features que harían a HollyShow destacar.

Leyenda — **Impacto**: ★ a ★★★ · **Esfuerzo**: S (días) / M (1–3 semanas) / L (1+ mes)

#### A. Inteligencia artificial (local y offline primero)

| Feature | Descripción | Impacto | Esfuerzo |
|---|---|---|---|
| **Detección de versículos por voz** | Escucha al predicador (micrófono o entrada de la consola), transcribe localmente con **whisper.cpp** (`whisper-rs`, modelo en español) y detecta referencias ("Juan tres dieciséis", "como dice Romanos 8"). La sugerencia aparece en **preview**; el operador la confirma con una tecla. Modo "auto-live" opcional. | ★★★ | L |
| Detección de citas/paráfrasis | Si el predicador *lee* un versículo sin decir la referencia, buscarlo por similitud de texto (FTS + embeddings locales) en la versión activa. | ★★ | L |
| **Subtítulos en vivo** | La misma transcripción como subtítulos de la predicación en la pantalla, en el stream (OBS) o en celulares. Accesibilidad para personas con discapacidad auditiva. | ★★★ | M (reutiliza lo anterior) |
| Traducción en vivo | Traducir los subtítulos a otro idioma para visitantes (modelo local o servicio en la nube opcional). | ★★ | M |
| Formateo inteligente de letras | Pegar una letra en bruto → detectar estrofas, coro repetido, puente; quitar acordes en línea; proponer secciones y arreglo. Heurísticas primero, IA opcional. | ★★ | S–M |
| Seguimiento automático de letra | Escuchar la música/voz y avanzar slides solos (experimental; el operador siempre puede intervenir). | ★ | L |
| Sugerencias de canciones | Por tema del sermón, tono, BPM, no repetidas recientemente (usa el historial). | ★ | M |

> Preocupaciones IA: consumo de CPU/GPU en equipos modestos (usar modelos `small`/`base` cuantizados, Metal en Mac), permisos de micrófono en macOS, privacidad (procesar todo localmente por defecto), y **nunca proyectar algo sin control humano** salvo que el operador lo active explícitamente.

#### B. Congregación conectada

| Feature | Descripción | Impacto | Esfuerzo |
|---|---|---|---|
| **Letras y Biblia en el celular de la congregación** | QR en pantalla → página web de solo lectura que sigue el slide en vivo: letra grande, modo oscuro, versión bíblica e idioma elegidos por cada persona. Ideal para personas con baja visión o visitantes de otro idioma. | ★★★ | M |
| Peticiones de oración / preguntas | Formulario desde el celular → bandeja moderada por el operador → mostrar en pantalla o enviar al pastor. | ★★ | M |
| Encuestas en vivo | Votación simple (ej. canción para cerrar, preguntas en estudios bíblicos) con resultados en pantalla. | ★ | M |

> Preocupación: decenas o cientos de celulares en el Wi-Fi de la iglesia. La vista de congregación debe ser liviana (SSE o WebSocket de solo lectura, sin imágenes pesadas), con límite de conexiones configurable y separada de los permisos de control.

#### C. Producción profesional e integraciones

| Feature | Descripción | Impacto | Esfuerzo |
|---|---|---|---|
| **Múltiples salidas con contenido distinto** | Pantalla principal (letra + fondo), pantallas laterales, monitor del lobby (anuncios en loop), stage display, salida OBS — cada una con su "look" sobre el mismo estado. | ★★★ | M |
| **API pública HTTP/WebSocket + módulo Bitfocus Companion** | Control desde **Stream Deck**, consolas y otras apps. Companion es el estándar en iglesias con producción. | ★★★ | M |
| Entrada/salida **MIDI y OSC** | Disparar slides desde secuencias de pistas (MultiTracks/Ableton/Playback) y mandar cues a luces o a otros sistemas. | ★★ | M |
| Cámara en vivo como fondo (IMAG) | Capturadora/webcam como capa de fondo con letra encima. | ★★ | M |
| Salida NDI | Para switchers de video sin capturadora (ver licencia del SDK). | ★★ | M–L |
| Control de luces (sACN/Art-Net) | Cues de iluminación sencillos ligados a slides o secciones. | ★ | L |
| Planning Center Services | Importar planes de culto, canciones y equipo (muy usado en iglesias de habla inglesa). | ★★ | M |

#### D. Planificación y equipo

| Feature | Descripción | Impacto | Esfuerzo |
|---|---|---|---|
| **Plantillas de culto** | Estructura que se repite cada domingo (bienvenida, alabanza ×4, ofrenda, prédica, anuncios) → crear el culto en segundos. | ★★★ | S |
| Temporizadores del culto | Cada ítem con duración prevista; el stage display muestra tiempo restante y avisa si se pasa ("faltan 2 min"). | ★★ | S–M |
| Modo ensayo | Recorrer el culto sin afectar la salida en vivo; grabar tiempos de cada slide para auto-avance opcional. | ★★ | M |
| Estadísticas | Canciones más/menos usadas, rotación, reporte CCLI exportable. | ★ | S |
| Equipo y turnos | Quién opera, quién canta, notificaciones (requiere nube, v2). | ★ | L |

#### E. Fiabilidad "de nivel profesional"

| Feature | Descripción | Impacto | Esfuerzo |
|---|---|---|---|
| **Computadora de respaldo en caliente** | Una segunda computadora en la LAN espeja el estado en tiempo real y puede tomar la salida con un clic si la principal falla. Muy pocos programas lo ofrecen. | ★★★ | M–L |
| **"Modo culto"** | Bloquea ediciones accidentales, pospone actualizaciones, avisa si las notificaciones del sistema están activas, evita que la pantalla se suspenda. | ★★★ | S |
| Panel de salud | FPS de la salida, frames perdidos, CPU/memoria, remotos conectados, estado de la red. | ★★ | S–M |
| Reporte de errores opt-in | Envío de crashes (ej. Sentry) sólo con consentimiento. | ★★ | S |

#### F. Experiencia moderna

- **Paleta de comandos** (Cmd/Ctrl+K) para todo: buscar canción, versículo, ejecutar acciones.
- **Deshacer/rehacer** en toda la app; arrastrar y soltar archivos, carpetas de fondos y letras.
- **Edición en vivo ("reflow")**: corregir un error de tipeo en el slide en vivo y que se actualice sin cortar.
- **Galería de temas** y fondos animados libres incluidos; importar/exportar temas como archivo para compartir entre iglesias.
- **Modo oscuro/claro**, UI escalable para pantallas pequeñas, accesibilidad (alto contraste, fuentes legibles).
- **Onboarding con contenido de ejemplo** y tour guiado; asistente para conectar el proyector y el remoto.
- **Sincronización en la nube opcional** (biblioteca, temas, cultos) para iglesias con varias sedes o para preparar el culto desde casa — siempre offline-first, la nube nunca es requisito para proyectar.
- **Colaboración remota** (v2): el líder de alabanza arma la lista desde su casa y aparece en la computadora de la iglesia.

#### Recomendación de prioridad

1. **Para v1.0** (bajo esfuerzo, alto impacto): plantillas de culto, modo culto, múltiples salidas, temporizadores, paleta de comandos, edición en vivo.
2. **Primer gran diferenciador post-v1:** letras/Biblia en el celular de la congregación + API/Companion.
3. **Segundo diferenciador:** detección de versículos por voz + subtítulos en vivo (local, en español). Es lo más novedoso del mercado y muy pocos competidores lo hacen bien en español.
4. **Para iglesias grandes:** respaldo en caliente, MIDI/OSC, NDI, Planning Center.

---

## 3. Arquitectura

```
┌──────────────────────────── App de escritorio (un proceso Tauri) ────────────────────────────┐
│                                                                                               │
│  ┌──────────────┐   ┌──────────────────┐   ┌──────────────────┐                               │
│  │ Ventana       │   │ Ventana Salida   │   │ Ventana Stage    │   ← WebViews (React)          │
│  │ Operador      │   │ (proyector)      │   │ (monitor escena) │                               │
│  └──────┬────────┘   └────────▲─────────┘   └────────▲─────────┘                               │
│         │ invoke/commands      │ eventos Tauri         │                                        │
│  ┌──────▼──────────────────────┴──────────────────────┴───────────────────────────────────┐   │
│  │                         NÚCLEO RUST (fuente única de verdad)                           │   │
│  │  PresentationState  ·  Playlist  ·  Biblioteca  ·  Biblia  ·  Temas  ·  Media          │   │
│  │  Bus de comandos (Command → State → Event broadcast con número de versión)            │   │
│  │  SQLite (rusqlite + FTS5)        Sistema de archivos (media, backups)                  │   │
│  └──────────────────────────────────────┬─────────────────────────────────────────────────┘   │
│                                         │                                                     │
│  ┌──────────────────────────────────────▼─────────────────────────────────────────────────┐   │
│  │  Servidor LAN embebido (axum + tokio): HTTP (sirve app remota) + WebSocket (protocolo) │   │
│  │  mDNS (hollyshow.local) · emparejamiento PIN/QR · rutas /output/* para OBS              │   │
│  └──────────────────────────────────────┬─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────┼─────────────────────────────────────────────────────┘
                                          │  Wi-Fi / LAN
                ┌─────────────────────────┼──────────────────────────┐
                ▼                         ▼                          ▼
        Celular operador          Tablet músicos              OBS (browser source)
        (React web app)           (modo acordes)              /output/lower-third
```

### 3.1 Decisiones clave

1. **El estado vive en Rust, no en React.** Las ventanas y los remotos son "vistas" que envían *comandos* y reciben *eventos*. Así operador, salida, stage y N remotos nunca se desincronizan.
2. **Comandos idempotentes con versión.** Cada estado tiene `version: u64`. Un comando "ir al slide 5 del ítem X" es seguro de repetir; "siguiente" lleva `expected_version` para evitar dobles saltos si dos personas tocan a la vez o si hay reintentos por red.
3. **Un único renderizador de slides compartido** (`packages/slide-renderer`): el mismo componente React dibuja la salida, el preview, las miniaturas, el stage display, la salida OBS y las miniaturas del remoto. Lo que ves en preview es exactamente lo que sale.
4. **Modelo de capas en la salida:** `fondo` → `media (video/imagen)` → `texto` → `overlay (lower third / alerta)` → `negro/logo`. Cada capa transiciona de forma independiente (cambiar letra sin reiniciar el video de fondo).
5. **Ventana de salida "tonta" y robusta:** no depende de la ventana del operador; si el operador se cuelga/recarga, la salida mantiene el último estado.
6. **La app remota se compila y se embebe en el binario** (`rust-embed`), no requiere internet ni servidor externo.

### 3.2 Stack

| Capa | Tecnología |
|---|---|
| Shell de escritorio | **Tauri 2** (multiventana, plugins oficiales: updater, dialog, fs, window-state, single-instance, log) |
| Núcleo | **Rust** + tokio |
| Servidor LAN | **axum** (HTTP + WebSocket), `mdns-sd` para descubrimiento, `qrcode` para QR |
| Base de datos | **SQLite** con `rusqlite` (o `sqlx`) + **FTS5**; migraciones versionadas |
| Tipos compartidos Rust↔TS | **specta / tauri-specta** (o `ts-rs`) — generar tipos TS desde Rust, nunca a mano |
| UI | **React 18 + TypeScript + Vite**, Tailwind + Radix/shadcn, **Zustand** sólo para estado de UI local, TanStack Query para consultas |
| Animaciones | CSS transitions/Web Animations API, **Framer Motion**; WebGL (regl o three.js) para shaders de transición |
| Remoto | React + Vite (app separada, mobile-first), mismo paquete de tipos/protocolo |
| i18n | i18next (español primero, inglés y portugués después) |
| Tests | `cargo test`, Vitest, Playwright (remoto y renderer en navegador) |
| CI/CD | GitHub Actions + `tauri-action` (builds firmados para macOS universal y Windows x64/ARM64) |
| Gestor de paquetes | pnpm workspaces + Cargo workspace |

### 3.3 Estructura del repositorio

```
hollyshow/
├── apps/
│   ├── desktop/                 # Tauri app
│   │   ├── src/                 # React: ventanas operador / salida / stage (rutas por ventana)
│   │   └── src-tauri/           # binario Tauri: comandos, ventanas, wiring
│   └── remote/                  # React web app para celular (se embebe en el binario)
├── crates/
│   ├── core/                    # dominio: estado, comandos, eventos, playlist (sin Tauri → testeable)
│   ├── storage/                 # SQLite, migraciones, FTS, backups
│   ├── bible/                   # parser de referencias en español, importadores (OSIS, USFM, Zefania)
│   ├── songs/                   # modelo de canción, split en slides, importadores (OpenLyrics, ChordPro…)
│   └── server/                  # axum, WebSocket, auth/pairing, mDNS
├── packages/
│   ├── protocol/                # tipos TS generados + cliente WebSocket con reconexión
│   ├── slide-renderer/          # componente React de renderizado de slides + transiciones
│   └── ui/                      # componentes compartidos
└── docs/
```

### 3.4 Protocolo del remoto (WebSocket, JSON, versionado)

```jsonc
// cliente → servidor
{ "type": "command", "id": "c-123", "cmd": { "kind": "next", "expectedVersion": 418 } }
{ "type": "command", "id": "c-124", "cmd": { "kind": "showBible", "ref": "Jn 3:16-18", "target": "live" } }
{ "type": "command", "id": "c-125", "cmd": { "kind": "blackout", "on": true } }

// servidor → clientes
{ "type": "ack",   "id": "c-123", "ok": true }
{ "type": "state", "version": 419, "live": { "itemId": "...", "slide": 3 }, "preview": {...}, "blackout": false }
{ "type": "event", "kind": "playlistChanged", "version": 420 }
```

- Al conectar: `hello` con versión de protocolo + token → servidor envía *snapshot* completo, luego sólo diffs/eventos.
- Heartbeat cada 5 s; reconexión exponencial; al reconectar se pide snapshot si la versión no coincide.
- Objetivo de latencia toque → proyector: **< 100 ms** en Wi-Fi normal.

---

## 4. Renderizado, efectos y salida

- **Auto-fit de texto:** algoritmo de búsqueda binaria del tamaño de fuente que cabe en el área segura, con tamaño mínimo/máximo por tema; calcular una vez por slide y cachear. Mismo tamaño para todos los slides de una canción (opción) para evitar "saltos".
- **Pre-render del siguiente slide** en una capa oculta para que la transición arranque sin tirones.
- **Transiciones:**
  - Nivel 1 (CSS/WAAPI): corte, fundido, crossfade, deslizar, zoom, desenfoque.
  - Nivel 2 (WebGL): biblioteca tipo *gl-transitions* (portada a GLSL propio, revisar licencia MIT de cada shader) aplicada sobre capturas de las capas.
  - Animación de texto: aparición por línea/palabra, "máquina de escribir" (con moderación).
  - Configurable por tema, por ítem y por slide.
- **Video de fondo:** recomendar/convertir a **H.264 MP4** (máxima compatibilidad WKWebView + WebView2). Loop sin corte (doble `<video>` alternado si hace falta).
- **Multi-monitor:** listar monitores (`available_monitors`), colocar ventana sin bordes al tamaño exacto del monitor elegido, recordar la elección por nombre/resolución, reaccionar a conectar/desconectar el proyector (volver a negro y reubicar, nunca mostrar la UI del operador en el proyector).
- **Salida OBS:** las rutas `/output/full` y `/output/lower-third` usan el mismo renderer con fondo transparente.

---

## 5. Preocupaciones y riesgos

### 5.1 Técnicas

| Riesgo | Detalle | Mitigación |
|---|---|---|
| **WebViews distintos por SO** | macOS usa WKWebView (Safari), Windows usa WebView2 (Chromium). Diferencias en CSS, códecs, rendimiento WebGL, fuentes. | Probar el renderer en ambos desde el día 1 (CI con builds para ambos); evitar CSS de vanguardia; H.264 para video; matriz de pruebas manual con proyectores reales. |
| **Pantalla completa en macOS** | El fullscreen nativo crea un "Space" separado, animaciones y problemas con el segundo monitor y la barra de menú. | Ventana sin bordes del tamaño del monitor + nivel de ventana alto (evitar fullscreen nativo); probar con "Displays have separate Spaces" activado/desactivado. |
| **Rendimiento de transiciones (60 fps)** | Textos grandes con sombras + video 1080p/4K + WebGL en equipos modestos de iglesia. | Animar sólo `opacity`/`transform`; `will-change`; perfiles de calidad (alta/media/baja); medir en hardware bajo (portátil Intel con gráficos integrados). |
| **Estabilidad en cultos largos** | Fugas de memoria tras 3–4 h (videos, imágenes, listeners). | Pruebas de resistencia ("soak test") automatizadas de 4 h ciclando slides; liberar recursos de media; monitorizar memoria en logs. |
| **Firewall y permisos de red** | Windows Defender muestra diálogo al abrir el puerto; macOS puede pedir permiso de "Red local" y del firewall. Si el usuario lo rechaza, el remoto "no funciona" sin explicación. | Asistente de primer uso que explica y verifica el puerto; pantalla de diagnóstico ("¿se ve el servidor desde el celular?"); app firmada para que el permiso persista; puerto configurable. |
| **Redes de iglesia problemáticas** | Wi-Fi de invitados con *client isolation* (los dispositivos no se ven entre sí), varias VLAN, IP que cambia. | Mostrar todas las IPs locales + `hollyshow.local` (mDNS) + QR; detectar y explicar el aislamiento; documentar "usar la red del staff"; opcional: hotspot del propio equipo. |
| **HTTPS en LAN** | Sin HTTPS no hay Service Worker/PWA instalable ni algunas APIs (wake lock, etc.) en el navegador del celular. Certificados autofirmados generan advertencias. | V1: HTTP + token (la app remota no necesita offline: si no hay red, no hay remoto). Evaluar después certificados locales o una app nativa ligera. Usar `Screen Wake Lock` sólo si está disponible. |
| **Seguridad del remoto** | Cualquiera en el Wi-Fi podría controlar la proyección. | Emparejamiento con PIN de 6 dígitos / QR con token de un solo uso → token de sesión; roles y permisos; lista de dispositivos conectados con botón "expulsar"; rate limiting; sólo escuchar en interfaces privadas; nunca exponer a internet. |
| **Concurrencia de remotos** | Dos personas pulsan "siguiente" a la vez → salto doble. | `expectedVersion` en comandos relativos; modo "un solo operador con control" opcional. |
| **Fuentes** | La fuente elegida en una PC puede no existir en otra (o en la ventana de salida). | Incluir fuentes libres (Google Fonts con licencia OFL) en la app; avisar si un tema usa una fuente ausente; fallback definido. |
| **Testing E2E de escritorio** | `tauri-driver` (WebDriver) no soporta macOS. | Lógica en Rust probada con unit tests; renderer y remoto probados en navegador con Playwright; E2E de escritorio sólo en Windows/Linux en CI + checklist manual en macOS. |
| **Crash recovery** | Si la app se cierra en pleno culto. | Autosave continuo del estado (playlist + slide actual); al reabrir, ofrecer "retomar donde estabas"; logs con `tauri-plugin-log`; panic hook que guarda el estado. |

### 5.2 Legales y de licencias

- **Textos bíblicos:** la **Reina-Valera 1960, NVI, NTV, LBLA, DHH** tienen copyright (Sociedades Bíblicas Unidas, Biblica, Tyndale, Lockman…). **No se pueden distribuir** sin licencia.
  - Incluir sólo versiones de **dominio público**: Reina-Valera 1909, Reina-Valera Antigua (1602/1865), KJV, etc.
  - Permitir que el usuario **importe** versiones que posea legalmente (formatos OSIS, USFM, Zefania XML, SWORD si es viable).
- **Letras de canciones:** proyectar letras requiere que la iglesia tenga licencia (ej. **CCLI**). La app no distribuye letras; ofrece campo CCLI, mostrar copyright en el último slide y reporte de uso.
- **"Clon" de Holyrics:** inspirarse en funcionalidades está bien; **no copiar** marca, iconos, textos, diseño visual ni código. Importar datos del usuario desde formatos de Holyrics sólo si el formato es documentado/exportable por el usuario (investigar; preferir que el usuario exporte a texto/OpenLyrics). Revisar que el nombre "HollyShow" no genere conflicto de marca.
- **Licencias de dependencias:** auditar con `cargo-deny` y `license-checker` (evitar GPL si se quiere distribuir cerrado). NDI SDK tiene licencia propietaria con condiciones de distribución.

### 5.3 Distribución

| Tema | macOS | Windows |
|---|---|---|
| Formato | `.dmg` con `.app` **universal** (Apple Silicon + Intel) | Instalador **NSIS** `.exe` (y/o `.msi`), x64 + ARM64 |
| Firma | **Apple Developer ID** (USD 99/año) + **notarización** obligatoria o Gatekeeper bloquea la app | Certificado de firma de código (Azure Trusted Signing es lo más económico hoy; OV/EV como alternativa). Sin firma: SmartScreen asusta a los usuarios |
| Runtime | WKWebView incluido en el SO (definir versión mínima de macOS, ej. 12+) | WebView2 viene en Windows 10/11 actualizados; incluir bootstrapper para equipos viejos |
| Actualizaciones | `tauri-plugin-updater` con firmas propias, canal estable/beta, **nunca actualizar en medio de un culto** (sólo al iniciar y con confirmación) | igual |
| Permisos | Red local, firewall; entitlements de sandbox si se publica en Mac App Store (no recomendado al inicio) | Regla de firewall en el instalador (opcional) |

---

## 6. Modelo de datos (resumen)

```
songs(id, title, alt_title, author, copyright, ccli, key, bpm, tags, theme_id, created_at, updated_at)
song_sections(id, song_id, kind[verse|chorus|bridge|...], label, order, lyrics, chords?)
song_arrangements(id, song_id, name, sequence[json: section ids])
songs_fts(title, lyrics, author)                     -- FTS5, texto normalizado sin acentos

bibles(id, code, name, language, license, copyright_text, is_public_domain,
       versification, sort_order, is_enabled, is_default, is_secondary)
canonical_books(osis_id, book_num, testament, is_deuterocanonical)       -- Gen, Exod, …, John, …
book_names(osis_id, language, name, abbrevs[json])                      -- "Juan", ["Jn","Jua"] / "John", ["Jn","Jhn"]
bible_books(bible_id, osis_id, display_name)                            -- libros presentes en cada versión
bible_verses(bible_id, osis_id, chapter, verse, text, markup[json])     -- cursivas, palabras de Jesús
versification_map(scheme, osis_id, chapter, verse, std_chapter, std_verse)  -- alinear versiones en paralelo
bible_fts(bible_id, text)                                              -- FTS5 por versión, sin acentos

themes(id, name, json)                               -- fondo, fuentes, márgenes, transiciones
media(id, path, kind, hash, duration, thumbnail)
playlists(id, name, date, items[json])               -- culto: referencias a canción/pasaje/media
usage_log(id, song_id, shown_at)                     -- reportes CCLI
devices(id, name, role, token_hash, last_seen)        -- remotos emparejados
settings(key, value)
```

- Media guardada por referencia + hash; opción "empaquetar culto" que copia todo a una carpeta/zip portable.
- Backups automáticos diarios de la base de datos (rotación).
- Exportar/importar biblioteca completa.

---

## 7. Hoja de ruta por fases

Estimaciones para 1 desarrollador a tiempo completo; ajustar según dedicación.

### Fase 0 — Fundaciones (1–2 semanas)
- Monorepo (pnpm + Cargo workspace), Tauri 2 + React + Vite, lint/format (clippy, rustfmt, ESLint, Prettier), CI que compila macOS y Windows.
- Generación de tipos Rust→TS, `crates/core` con estado/comandos/eventos y tests.
- **Spike de riesgo:** ventana de salida en segundo monitor en macOS y Windows + transición fundido con video de fondo a 60 fps. Si esto no funciona bien, se detecta aquí.

### Fase 1 — Núcleo de presentación (3–4 semanas)
- Ventanas operador / salida; selección de monitor; preview/live; negro/limpiar/logo.
- `slide-renderer` con auto-fit, temas básicos, transiciones CSS.
- SQLite + migraciones; CRUD de canciones con secciones y arreglo; búsqueda FTS sin acentos.
- Playlist del culto con autosave; atajos de teclado.

### Fase 2 — Control remoto (2–3 semanas) ⭐
- Servidor axum embebido, WebSocket, snapshot + eventos versionados.
- App remota: emparejamiento por QR/PIN, siguiente/anterior, lista de slides, playlist, negro.
- mDNS, pantalla de diagnóstico de red, lista de dispositivos conectados.
- Pruebas con varios celulares (iOS Safari, Android Chrome) y reconexión al perder Wi-Fi.

### Fase 3 — Biblia multiversión (3–4 semanas)
- Modelo canónico de libros (OSIS) + nombres/abreviaturas por idioma.
- Importadores (OSIS/USFM/Zefania/OpenSong) + varias Biblias de dominio público incluidas (ej. RVR1909, Reina-Valera Antigua, KJV, Almeida antigua).
- Gestor de versiones: instalar, activar, ordenar, versión por defecto y secundaria.
- Parser de referencias en español (abreviaturas, rangos, capítulos completos, libros con número: "1 Juan", "1Jn", "I Juan").
- Cambio de versión en vivo; modo paralelo 2–4 versiones con división sincronizada; mapeo de versificación.
- Búsqueda por texto por versión o en todas; división de versículos largos.
- Búsqueda bíblica y selector de versión desde el remoto (rol "Pastor", vista "lector").

### Fase 4 — Medios y efectos (2–3 semanas)
- Fondos de imagen/video, biblioteca de media con miniaturas.
- Modelo de capas completo; transiciones WebGL; animación de texto; Ken Burns.
- Editor visual de temas con vista previa en vivo.

### Fase 5 — Escenario y streaming (2 semanas)
- Stage display configurable (actual, siguiente, reloj, timer, mensajes).
- Acordes + transposición; modo "Músico" en el remoto.
- Salidas para OBS (`/output/*`) con transparencia; lower thirds.
- Alertas superpuestas, contadores regresivos, anuncios.

### Fase 6 — Producción (2–3 semanas)
- Firma + notarización macOS, firma Windows, instaladores, auto-updater.
- Importadores restantes, backups, crash recovery, logs exportables para soporte.
- i18n, onboarding, documentación de usuario.
- **Beta en 2–3 iglesias reales** durante cultos con feedback.

> Incluir en Fases 1–6 los "rápidos" de §2.5: plantillas de culto, modo culto, temporizadores, paleta de comandos, edición en vivo y múltiples salidas.

**Total estimado MVP sólido (Fases 0–3):** ~2,5–3 meses. **v1.0 completa (0–6):** ~5–5,5 meses.

### Fase 7 — Diferenciadores (post v1.0, iterativo)
- **7a. Congregación conectada (3–4 semanas):** vista de solo lectura en celulares, peticiones de oración, encuestas.
- **7b. Integraciones (3–4 semanas):** API pública documentada, módulo Bitfocus Companion, MIDI/OSC, Planning Center.
- **7c. IA de voz (6–8 semanas):** transcripción local con whisper.cpp, detección de referencias en español, subtítulos en vivo, luego paráfrasis y traducción.
- **7d. Profesional (4–6 semanas):** computadora de respaldo en caliente, NDI, cámara en vivo, sincronización en la nube opcional.

---

## 8. Calidad y pruebas

- **Rust:** unit tests del parser bíblico (tabla extensa de referencias válidas/ inválidas), división de slides, comandos/estado (property tests con `proptest` para concurrencia de comandos).
- **Frontend:** Vitest para lógica; Playwright para el remoto y el renderer (capturas de pantalla comparativas de slides — *visual regression*).
- **Integración:** test que levanta el servidor y simula N remotos enviando comandos concurrentes.
- **Soak test:** 4 h de cambios de slide con video de fondo midiendo memoria y fps.
- **Checklist manual por release:** proyector real en macOS (Intel + Apple Silicon) y Windows 10/11, conexión/desconexión de monitor en caliente, Wi-Fi que se cae, permisos de firewall en instalación limpia.

---

## 9. UX — notas para el operador

- Flujo principal en una sola pantalla: **Playlist (izq.) · Slides del ítem (centro) · Preview/Live (der.) · Búsqueda rápida (Ctrl/Cmd+K)** para canciones y versículos.
- Indicador grande y claro de **qué está en vivo** (borde rojo) vs preview.
- Todo accionable por teclado y por pedal (las pedaleras Bluetooth emulan teclas).
- Modo "operador novato": menos opciones visibles.
- El remoto: botones grandes, usable con una mano, modo oscuro, vibración al confirmar, indicador de conexión siempre visible.

---

## 10. Decisiones abiertas

1. **Modelo de negocio:** ¿gratuito/open source, freemium o pago? Afecta licencias de dependencias, firma y distribución.
2. **Versiones bíblicas:** ¿intentar licenciar RVR1960/NVI o depender de que el usuario importe las suyas?
3. **Versión mínima de SO:** propuesta macOS 12+ y Windows 10 (21H2)+.
4. **¿Linux?** Tauri lo soporta casi gratis; decidir si se publica oficialmente (WebKitGTK tiene sus propias diferencias).
5. **Importación desde Holyrics:** confirmar qué puede exportar Holyrics (texto, backup) y si es legítimo/práctico leerlo.
6. **Remoto como PWA (HTTP) vs app nativa:** empezar con web; reevaluar tras la beta.
7. **Sincronización en la nube:** ¿necesaria para iglesias con varias computadoras/sedes?
8. **IA:** ¿sólo local (privado, gratis, más CPU) o también servicios en la nube (más preciso, costo por uso, requiere internet)? Propuesta: local por defecto, nube opcional.
9. **Orden de diferenciadores post-v1:** congregación conectada vs. IA de voz vs. integraciones de producción — depende del perfil de las iglesias objetivo.

---

## 11. Primeros pasos concretos

1. Crear el monorepo y el esqueleto Tauri 2 + React (Fase 0).
2. Hacer el **spike de salida en segundo monitor + transición + video** en ambos SO.
3. Definir el protocolo de comandos/eventos en `crates/core` con tests.
4. Construir la ventana de salida y un remoto mínimo ("siguiente/anterior") para validar la arquitectura de extremo a extremo antes de invertir en features.
