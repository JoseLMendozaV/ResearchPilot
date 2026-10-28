# Revisión inicial de licencias

Revisión realizada el 28 de octubre de 2026. No sustituye asesoría jurídica; debe repetirse antes de cada distribución.

## Dependencias instaladas

Se inspeccionaron los metadatos de todos los paquetes instalados por npm y Cargo.

- Predominan MIT y Apache-2.0, además de BSD, ISC, Zlib, Unicode-3.0 y licencias permisivas equivalentes.
- Lightning CSS y algunos componentes del árbol web declaran MPL-2.0; exige conservar disponible el código fuente de archivos MPL modificados, no relicenciar toda la aplicación.
- `r-efi` ofrece una elección permisiva MIT/Apache-2.0 además de LGPL; se usará bajo la opción permisiva aplicable.
- No se detectaron dependencias AGPL ni GPL-only instaladas.
- La licencia propia de ResearchPilot aún debe decidirse antes de distribuir código o binarios.
- Dependencias incorporadas en Fase 2: PDF.js (Apache-2.0), `lopdf` (MIT), `csv` (MIT/Unlicense) y el diálogo oficial de Tauri (Apache-2.0/MIT).
- Fase 3 (Python, no incluidas en el binario; se instalan en el entorno del extractor tras confirmación): `pdfplumber` 0.11.10 (MIT), `pdfminer.six` (MIT), `pypdfium2` (Apache-2.0 o BSD-3-Clause; incluye binarios de PDFium bajo licencias BSD/Apache), `Pillow` (MIT-CMU), `charset-normalizer` (MIT), `cryptography` (Apache-2.0 o BSD-3-Clause). Tesseract (Apache-2.0) y GROBID (Apache-2.0) no se distribuyen: los instala y ejecuta el usuario.
- Fase 9: `zip` 9 (MIT) para respaldos. En Linux, `keyring` añade Secret Service vía `dbus-secret-service` (MIT/Apache-2.0) y cifrado en Rust puro (`aes`, `cbc`, `hkdf`, `num`: MIT/Apache-2.0); requiere la biblioteca del sistema `libdbus-1` (AFL-2.1 o GPL-2.0, enlazada dinámicamente, no se distribuye). No se compilan en macOS. Zotero no se distribuye: solo se lee una copia de la base local del usuario.
- Fase 7: `rust_xlsxwriter` 0.101 (MIT/Apache-2.0); `calamine` 0.37 (MIT) solo en pruebas.
- Fase 5: `keyring` 3.6 (MIT/Apache-2.0) con el backend nativo del Llavero de macOS.
- Fase 4: `ureq` 3.5 (MIT/Apache-2.0) con `rustls` (Apache-2.0/MIT/ISC) y `ring`/`webpki` (licencias permisivas tipo ISC/Apache). Ollama no se distribuye; los modelos que el usuario descargue tienen licencias propias que la interfaz recomienda revisar.

## Componentes previstos

| Componente | Licencia verificada | Decisión preliminar |
|---|---|---|
| PDF.js | Apache-2.0 | Apto, conservar LICENSE/NOTICE |
| GROBID | Apache-2.0; documentación CC0 y datos anotados CC-BY | Apto como servicio local opcional; revisar artefactos/datos distribuidos |
| Ollama | MIT | Apto; los modelos tienen licencias separadas que deben mostrarse y validarse |
| pdfplumber | MIT | Apto |
| pdfminer.six | MIT, según documentación de pdfplumber | Apto |

Fuentes primarias: [PDF.js LICENSE](https://github.com/mozilla/pdf.js/blob/master/LICENSE), [GROBID](https://github.com/grobidOrg/grobid), [Ollama LICENSE](https://github.com/ollama/ollama/blob/main/LICENSE), [pdfplumber](https://github.com/jsvine/pdfplumber).

## Exclusiones preventivas

- No incorporar MuPDF/PyMuPDF ni Ghostscript sin una decisión explícita sobre AGPL o licencia comercial.
- No asumir que la licencia de Ollama cubre modelos descargados; cada modelo requiere evaluación propia.
- No redistribuir la plantilla Excel ni fixtures científicos hasta verificar derechos de uso.
- Los paquetes Python, OCR, índice vectorial y generador XLSX se auditarán antes de agregarlos al lockfile.

## Obligaciones de distribución

Generar un inventario SBOM/licencias desde los lockfiles, incluir avisos requeridos, documentar versiones exactas y conservar el origen/licencia de fixtures, modelos y datos. Firma y notarización son pasos separados y no se declaran realizadas.
