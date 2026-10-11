# Biblias incluidas: fuentes y licencias

Solo se incluyen textos de **dominio público**. Ninguna traducción con derechos vigentes (RVR1960, NVI, NTV, LBLA, DHH,
TLA, etc.) se distribuye con la aplicación; el usuario puede importar sus propios archivos (OSIS, USFM, Zefania, OpenSong,
JSON, CSV) desde «Versiones de la Biblia».

| Código    | Nombre                    | Idioma | Versículos | Licencia        | Archivo          |
| --------- | ------------------------- | ------ | ---------- | --------------- | ---------------- |
| `RVR1909` | Reina-Valera 1909         | es     | 31 084     | Dominio público | `rvr1909.hsb.gz` |
| `KJV`     | King James Version (1769) | en     | 31 102     | Dominio público | `kjv.hsb.gz`     |
| `RV1865`  | Reina-Valera 1865         | es     | 31 098     | Dominio público | `rv1865.hsb.gz`  |

## Origen

Los tres textos se tomaron de la colección de módulos de CrossWire/SWORD publicada en JSON por el repositorio
`scrollmapper/bible_databases` (GitHub, código MIT; los textos conservan la licencia del módulo de origen):

| Código    | Archivo de origen (rama `master`) | Módulo      | Descripción del módulo                                        | SHA-256 del archivo de origen                                      |
| --------- | --------------------------------- | ----------- | ------------------------------------------------------------- | ------------------------------------------------------------------ |
| `RVR1909` | `formats/json/SpaRV.json`         | `SpaRV`     | La Santa Biblia Reina-Valera (1909)                           | `94675a8b71785e4468918227b2496f9c7d407be658190e47795bdd5daade953d` |
| `KJV`     | `formats/json/KJV.json`           | `KJV`       | King James Version (1769)                                     | `f0b09dc49dfb97bb84f03aae1fbf026485048c3cab31a7a41017e2d86ac1d11c` |
| `RV1865`  | `formats/json/SpaRV1865.json`     | `SpaRV1865` | La Santa Biblia Reina-Valera (1865) con arreglos ortográficos | `bce26de39969fbf16d7970ba29be7724ac738c5ac7639aaa55ea3f20819d75bb` |

URL base: `https://raw.githubusercontent.com/scrollmapper/bible_databases/master/`.

Notas de licencia:

- **RVR1909**: edición de la Sociedad Bíblica Británica y Extranjera de 1909; texto anterior a 1929, dominio público.
  No confundir con la **RVR1960**, que tiene derechos vigentes (Sociedades Bíblicas Unidas) y **no** se incluye.
- **KJV**: texto estándar de 1769 (Oxford). Dominio público en todo el mundo salvo el Reino Unido, donde la Corona
  mantiene una «letters patent» perpetua que regula su impresión; la patente no restringe su uso en una aplicación de
  proyección fuera de la impresión comercial en el Reino Unido. Si se distribuyera allí, conviene consultar.
- **RV1865**: texto de la revisión de 1865 (anterior a 1929, dominio público). El módulo de origen declara «arreglos
  ortográficos» hechos por sus compiladores (modernización de la ortografía); se distribuye tal cual. **No es** la
  «Reina-Valera Antigua» de 1602 (Purificada); si en el futuro se verifica una fuente de dominio público de esa edición,
  se añadirá como un `.hsb.gz` más (basta con dejarlo en esta carpeta).
- El proyecto no ha podido descargar desde eBible.org (el entorno de build bloquea ese dominio); si se desea sustituir
  alguna fuente por la de eBible.org (`spaRV1909_usfm.zip`, `engkjv_usfm.zip`), el importador USFM ya está listo.

## Formato `.hsb.gz`

Texto UTF-8 comprimido con gzip: cabecera `#hsbible 1`, líneas `clave=valor` (código, nombre, idioma, licencia, copyright,
dominio público, versificación), una línea en blanco y un versículo por línea `OSIS<TAB>texto[<TAB>marcas]` en orden
canónico. Lo carga `hollyshow_bible::import::HsbImporter` y `hollyshow_bible::install_bundled` lo instala en el
primer arranque. Cada Biblia pesa unos 1,3 MB comprimida (≈ 4 MB el conjunto).

## Verificación

`cargo test -p hollyshow-bible --test bundled` comprueba:

- KJV: **exactamente 31 102** versículos, ninguno ausente ni fuera del canon estándar de 66 libros, en orden canónico.
- RVR1909 y RV1865: lo que falta son solo los huecos documentados abajo.
- Sin marcas HTML/tabuladores/saltos de línea en el texto, y texto sin espacios sobrantes.
- Búsqueda de texto completo por versión y lectura a través de los traits de `storage-api`.

### Huecos de la fuente (versificación propia de la Reina-Valera)

Los módulos SWORD rellenan con versículos **vacíos** las posiciones donde la Reina-Valera numera o une versículos de otra
forma que el estándar KJV. Los importadores descartan los versículos vacíos; en el modo paralelo se muestra «—» en esa
columna. El texto no se pierde: queda unido al versículo vecino.

`RVR1909` (18): Nm 12:16, Nm 29:40, 1 S 23:29, 2 S 20:26, 2 Cr 33:25, Job 35:16, Job 38:39-41, Job 40:20-24, Os 11:12,
Jon 1:17, Hch 19:41, 2 Co 13:14.

`RV1865` (4): 1 Cr 3:24, 2 Cr 33:25, Sal 13:6, Jer 39:18.

Consecuencia conocida para `RVR1909`: en Nm 13, Nm 30, 1 S 24, Job 39-40, Os 12 y Jon 2 el texto de la columna
española está desplazado un versículo (o más, en Job) respecto a la KJV, porque la fuente conserva la numeración propia
de la RV de 1909 dentro de una rejilla KJV. Para esas pocas perícopas la vista paralela con la KJV queda desalineada.
La versificación `kjv` declarada es la de la rejilla del módulo. Corregirlo exige una tabla de versificación
«Reina-Valera 1909» con partición de los versículos unidos (pendiente; `RV1865` sí sigue la numeración KJV y sirve como
alternativa alineada).

## Regenerar

```bash
D=/ruta/a/descargas   # contiene los JSON de origen
cargo build -p hollyshow-bible --example pack_bible --release
P=target/release/examples/pack_bible
R=apps/desktop/src-tauri/resources/bibles
$P $D/SpaRV.json     $R/rvr1909.hsb.gz RVR1909 "Reina-Valera 1909" es true "Public Domain" "Reina-Valera 1909. Dominio público." kjv
$P $D/KJV.json       $R/kjv.hsb.gz     KJV     "King James Version (1769)" en true "Public Domain" "King James Version (1769). Public Domain." kjv
$P $D/SpaRV1865.json $R/rv1865.hsb.gz  RV1865  "Reina-Valera 1865" es true "Public Domain" "Reina-Valera 1865. Dominio público." kjv
```

(`pack_bible` importa cualquier formato soportado, comprueba el canon e imprime los versículos que faltan.)

La tabla de versículos por capítulo del canon estándar (`crates/bible/src/canon_data.rs`) se generó de la KJV:
recorrer `books[i].chapters[j].verses` y escribir `len(verses)` por capítulo.

## Integración en la aplicación (builder del shell)

1. Añadir los recursos a `tauri.conf.json`: `"bundle": { "resources": ["resources/bibles/*.hsb.gz"] }`.
2. En el primer arranque, resolver el directorio (`app.path().resolve("resources/bibles", BaseDirectory::Resource)`) y
   llamar a `hollyshow_bible::install_bundled(&dir, &sink)` con un adaptador de `hollyshow_storage_sqlite::Store`
   (ejemplo en la documentación de `BibleSink`). Es idempotente; no pisa las preferencias del usuario.
