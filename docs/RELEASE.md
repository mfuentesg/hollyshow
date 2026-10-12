# Publicar una versión de HollyShow

## Flujo normal

1. Sube la versión en un solo lugar: `version` de `[workspace.package]` en `Cargo.toml`, y la misma en
   `apps/desktop/src-tauri/tauri.conf.json` y los `package.json`. `scripts/check-version.mjs` (CI y release) falla si no coinciden con el tag.
2. Commit en `master` y espera a que CI (`.github/workflows/ci.yml`) esté en verde.
3. Crea y sube el tag:

   ```sh
   git tag v1.2.0          # estable
   git tag v1.3.0-beta.1   # prerelease (cualquier tag con guion)
   git push origin v1.2.0
   ```

4. `release.yml` verifica la versión, ejecuta CI como puerta de calidad y compila en paralelo:
   macOS universal (`aarch64` + `x86_64`: `.dmg` y `.app.tar.gz`), Windows x64 (`.exe` NSIS y `.msi`, con WebView2 embebido) y
   Linux (`.deb`, `.AppImage`). La app web del remoto (`apps/remote`) se compila antes porque queda embebida.
5. La release se **publica automáticamente** (no borrador) para que
   `https://github.com/mfuentesg/hollyshow/releases/latest/download/latest.json` resuelva. Los tags `-beta`/`-rc` se marcan como
   prerelease y no los toma `/releases/latest`.

También puedes lanzarlo a mano desde Actions > Release > Run workflow (usa la versión de los archivos del proyecto).

## Secretos de GitHub necesarios

Ajustes > Secrets and variables > Actions > New repository secret:

| Secreto                              | Para qué                                                                                                                                                                                                                                                                                                                                                                    |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | Clave privada minisign que firma los artefactos del actualizador. **Necesaria para releases publicados**: con ella el release activa `createUpdaterArtifacts` (vía `tauri.release.conf.json`) y publica `latest.json`. Sin ella el workflow igual genera los instaladores, pero sin actualizaciones firmadas y deja el release como borrador. Ver [SIGNING.md](SIGNING.md). |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Contraseña de esa clave (vacía si no tiene).                                                                                                                                                                                                                                                                                                                                |

La clave **pública** ya está en `tauri.conf.json` (`plugins.updater.pubkey`). La privada **nunca** se sube al repositorio.
Si hay que regenerar el par: `pnpm --filter desktop tauri signer generate -w ~/.tauri/hollyshow.key`, actualiza `pubkey` y los
secretos. Las apps ya instaladas con la clave antigua no podrán actualizarse automáticamente (deben reinstalarse), así que guarda la clave en un gestor de contraseñas.

## Actualizador

`tauri.conf.json` ya declara el endpoint `.../releases/latest/download/latest.json`; `tauri-action` genera y sube `latest.json`
(`includeUpdaterJson`) con las firmas `.sig` de cada plataforma. El shell ya registra `tauri-plugin-updater` y
`tauri-plugin-process` (`src-tauri/src/updater.rs`, permisos en `capabilities/updater.json`, solo para la ventana del operador):
comprueba a los 30 s de arrancar y cada 6 h, descarga en segundo plano y nunca reinicia con el modo culto activo ni con una salida en
vivo (se instala al salir).

## Firma de código (opcional pero recomendada)

Sin firma, macOS muestra «desarrollador no identificado» y Windows SmartScreen advierte. Los builds funcionan igualmente.

### macOS: Developer ID y notarización

1. Cuenta del Apple Developer Program. Crea un certificado «Developer ID Application» en developer.apple.com.
2. Expórtalo desde Acceso a Llaveros como `.p12` con contraseña y conviértelo: `base64 -i cert.p12 | pbcopy`.
3. Crea una contraseña específica de app en appleid.apple.com.
4. Secretos: `APPLE_CERTIFICATE` (base64 del .p12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`
   (p. ej. `Developer ID Application: Nombre (TEAMID)`), `APPLE_ID`, `APPLE_PASSWORD` (la contraseña de app), `APPLE_TEAM_ID`.
5. Descomenta el bloque de macOS en el paso `tauri-action` de `release.yml`. Con esas variables tauri-action firma con hardened runtime y notariza.

### Windows: Azure Trusted Signing

1. Crea una cuenta de Trusted Signing en Azure, valida la identidad de la organización y crea un perfil de certificado.
2. Registra una aplicación de Entra ID con el rol «Trusted Signing Certificate Profile Signer»; anota `AZURE_CLIENT_ID`,
   `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`.
3. Añade en `bundle.windows` de `tauri.conf.json` un `signCommand` que invoque `trusted-signing-cli` (`cargo install trusted-signing-cli` en el workflow) con tu endpoint, cuenta y perfil, y `%1` como archivo.
4. Descomenta los secretos de Azure en `release.yml`.
   Alternativa: certificado OV/EV `.pfx` con `certificateThumbprint` o `signCommand` con `signtool`.

## Puntos de verificación antes de etiquetar

- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm lint && pnpm typecheck && pnpm test`.
- `node scripts/check-version.mjs`, `pnpm gen:types` sin diff, `pnpm -r test`, `pnpm --filter remote build` y `pnpm --filter desktop tauri build` (Linux: `.deb` y `.AppImage` en `target/release/bundle`).
- Remoto: emparejar con PIN/QR (un solo uso, caduca), probar con HTTPS activado (aviso del certificado autofirmado) y que el informe CCLI genere datos.
- Lista de verificación manual en macOS (no hay `tauri-driver`): arranque, proyector, QR, remoto.
- Compatibilidad: macOS 12+, Windows 10/11 x64, Linux con WebKitGTK 4.1.
