# Publicar una versión y actualizaciones automáticas

ResearchPilot usa el actualizador oficial de Tauri 2 (`tauri-plugin-updater`). La app consulta
`https://github.com/JoseLMendozaV/ResearchPilot/releases/latest/download/latest.json` y solo instala
paquetes cuya firma coincide con la clave pública incluida en `src-tauri/tauri.conf.json`.

## Claves

| Elemento | Dónde vive | Nota |
|---|---|---|
| Clave pública | `src-tauri/tauri.conf.json` → `plugins.updater.pubkey` | Pública; va en el repositorio |
| Clave privada | `~/.tauri/researchpilot-updater.key` (permisos 600) | **Nunca** en el repositorio. Haz una copia de seguridad cifrada |
| Contraseña de la clave | Llavero de macOS, servicio `researchpilot-updater-key-password` | Ver con `security find-generic-password -s researchpilot-updater-key-password -w` |

Si se pierde la clave privada, las versiones instaladas no aceptarán actualizaciones firmadas con otra clave:
habría que publicar una versión con la nueva clave pública e instalarla manualmente una vez.

Esta firma es independiente de la firma y notarización de Apple, que siguen sin realizarse.

## Publicar

1. Sube la versión en `package.json`, `src-tauri/Cargo.toml` y `src-tauri/tauri.conf.json`.
2. Ejecuta las pruebas (`npm run test`, `cargo test`, pruebas Python) y haz commit y push a `main`.
3. Escribe las notas de la versión en un archivo Markdown.
4. Ejecuta:

```bash
scripts/release.sh notas.md            # compila y prepara dist-release/vX.Y.Z
scripts/release.sh notas.md --publish  # además crea el release en GitHub
```

El script genera en `dist-release/vX.Y.Z/`:

- `ResearchPilot_X.Y.Z_aarch64.dmg` — instalación manual.
- `ResearchPilot_X.Y.Z_aarch64.app.tar.gz` y su `.sig` — paquete de actualización firmado.
- `latest.json` — versión, notas, fecha y URL del paquete en el release del tag.
- `SHA256SUMS.txt`.

El release publicado debe quedar marcado como *latest* para que `releases/latest/download/latest.json` apunte a él.

## Firma de código en macOS

- `bundle.macOS.signingIdentity = "-"` firma el paquete `.app` completo con firma *ad hoc* y runtime reforzado. Sin ella, solo el binario lleva la firma del enlazador y macOS declara «dañada» la app descargada (con atributo de cuarentena).
- `scripts/release.sh` ejecuta `codesign --verify --deep --strict` sobre la app compilada y sobre la copia dentro del DMG, y aborta si la firma no es válida.
- La firma *ad hoc* no evita el aviso de desarrollador no identificado. Para eliminarlo hacen falta un certificado **Developer ID Application** (Apple Developer Program) y la **notarización**: `signingIdentity` con ese certificado más `APPLE_ID`, `APPLE_PASSWORD` y `APPLE_TEAM_ID` en el entorno del build.

## Comportamiento en la app

- Configuración → Actualizaciones → **Buscar actualizaciones**: muestra versión y notas; descarga, verifica e instala solo tras confirmar; luego ofrece reiniciar.
- **Buscar al abrir la app**: desactivado por defecto; nunca se consulta automáticamente en modo solo local.
- La consulta solo descarga `latest.json` de GitHub: no envía documentos ni datos del usuario.
- Actualizar reemplaza la aplicación; la base de datos, la biblioteca y las claves del Llavero no se tocan, y las migraciones se aplican al abrir.

Las versiones anteriores a la 0.6.1 no incluyen el actualizador: hay que instalar la 0.6.1 manualmente una vez.
