# Firma de versiones de HollyShow

Hay **dos firmas distintas** y conviene no confundirlas:

| Firma | Para qué sirve | ¿Obligatoria? |
|---|---|---|
| **Firma del actualizador** (Tauri, Ed25519) | Que la app sólo acepte actualizaciones publicadas por nosotros. | **Sí**, para que funcione la actualización automática. |
| **Firma de código** (Apple Developer ID / certificado de Windows) | Que macOS y Windows no muestren advertencias de "desarrollador no identificado" / SmartScreen. | No para compilar, pero muy recomendada para distribuir. |

---

## 1. Claves de firma del actualizador

Las actualizaciones automáticas van firmadas. Para generar un par de claves nuevo:

    pnpm tauri signer generate -w ~/.tauri/hollyshow.key

Esto crea:
- `~/.tauri/hollyshow.key` — clave privada (NUNCA al repo)
- `~/.tauri/hollyshow.key.pub` — clave pública
- la contraseña que elijas durante la generación

Después:
1. Copia el contenido de `hollyshow.key.pub` en `plugins.updater.pubkey` de `apps/desktop/src-tauri/tauri.conf.json`.
2. En GitHub → Settings → Secrets and variables → Actions, crea:
   - `TAURI_SIGNING_PRIVATE_KEY` = contenido de `hollyshow.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = la contraseña
3. Guarda la clave privada y la contraseña en un gestor de contraseñas. Si se pierden, las instalaciones existentes no podrán recibir actualizaciones (habría que reinstalar manualmente con la nueva clave pública).

> Nota: durante el desarrollo inicial se configuró una clave pública provisional. Al generar tu propio par, reemplázala (paso 1) **antes del primer release público**; las versiones firmadas con la clave anterior no podrán actualizarse a las firmadas con la nueva.

Comprobar que todo está listo:
- `tauri.conf.json` tiene `bundle.createUpdaterArtifacts: true`, `plugins.updater.pubkey` con tu clave pública y el endpoint `https://github.com/mfuentesg/hollyshow/releases/latest/download/latest.json`.
- Los dos secretos existen en GitHub (los valores no se pueden leer después de guardarlos; eso es normal).

---

## 2. Firma de código en macOS (opcional, recomendada)

Sin ella, Gatekeeper bloquea la app al abrirla ("no se puede verificar el desarrollador"); el usuario debe hacer clic derecho → Abrir.

1. Cuenta en el **Apple Developer Program** (USD 99/año).
2. En Xcode o en developer.apple.com crea un certificado **Developer ID Application** y expórtalo desde Llavero como `.p12` con contraseña.
3. Convierte el `.p12` a base64: `base64 -i certificado.p12 | pbcopy`
4. Crea una **contraseña específica de app** en appleid.apple.com (para la notarización).
5. Secretos en GitHub → Actions:
   - `APPLE_CERTIFICATE` = el `.p12` en base64
   - `APPLE_CERTIFICATE_PASSWORD` = contraseña del `.p12`
   - `APPLE_SIGNING_IDENTITY` = p. ej. `Developer ID Application: Tu Nombre (TEAMID)`
   - `APPLE_ID` = tu correo de Apple ID
   - `APPLE_PASSWORD` = la contraseña específica de app
   - `APPLE_TEAM_ID` = tu Team ID (10 caracteres)
6. Descomenta esas variables en el paso de `tauri-action` de `.github/workflows/release.yml`. Tauri firma y **notariza** automáticamente.

## 3. Firma de código en Windows (opcional, recomendada)

Sin ella, SmartScreen muestra "Windows protegió su PC" hasta que el instalador gane reputación.

Opciones:
- **Azure Trusted Signing** (la más económica hoy): crea una cuenta de Trusted Signing y un perfil de certificado en Azure; configura `bundle.windows.signCommand` en `tauri.conf.json` con la herramienta `trusted-signing-cli` y agrega como secretos `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`.
- **Certificado OV/EV** de una autoridad (DigiCert, Sectigo…): configura `bundle.windows.certificateThumbprint`, `digestAlgorithm: "sha256"` y `timestampUrl`, e importa el `.pfx` en el runner de Windows desde un secreto en base64 antes del build.

Detalles actualizados: https://v2.tauri.app/distribute/sign/windows/ y https://v2.tauri.app/distribute/sign/macos/

---

## 4. Publicar una versión

1. Sube la versión en los tres lugares (deben coincidir): `apps/desktop/src-tauri/tauri.conf.json` (`version`), `apps/desktop/src-tauri/Cargo.toml` y `apps/desktop/package.json`.
2. Commit y tag:

       git commit -am "Release v0.2.0"
       git tag v0.2.0
       git push origin master --tags

3. El workflow **Release** compila macOS (universal), Windows y Linux, firma las actualizaciones y publica el release con `latest.json`.
   - Tags `vX.Y.Z` → release estable (las apps instaladas lo detectan).
   - Tags `vX.Y.Z-beta.N` → prerelease (canal beta).
4. Revisa el release en GitHub, edita las notas (son las que ve el usuario en "Ver novedades").

### Lista de verificación antes de un release público
- [ ] Clave pública propia en `tauri.conf.json` y secretos del actualizador en GitHub.
- [ ] Versión actualizada en los tres archivos.
- [ ] CI en verde en `master`.
- [ ] (Recomendado) Secretos de firma de Apple y Windows configurados.
- [ ] Probar la actualización: instalar la versión anterior y verificar que aparece el aviso "nueva versión".
