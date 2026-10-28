# Cierre de Fase 3 — Extracción científica

## Objetivos implementados

- **Sidecar Python aislado** (`python/researchpilot_worker`): proceso hijo con entorno vaciado, `-B -E -s`, directorio de trabajo propio, límite de CPU, tiempo máximo y protocolo JSON por líneas versionado (`PROTOCOL_VERSION = 1`).
- **Cola persistente** (`ingestion_jobs`): tareas `scientific_extraction` y `extractor_setup` con progreso por etapa, pausa, reanudación, cancelación, reintento automático ante caídas (una vez) y manual, y recuperación tras cierre inesperado.
- **Extracción por página con diseño** (pdfplumber, MIT): palabras con posición → líneas → orden de lectura con detección de dos columnas → bloques tipados (encabezado, párrafo, leyenda, referencia, cabecera/pie) con bounding box y sección canónica.
- **OCR local opcional** (Tesseract, Apache-2.0, instalado por el usuario): solo para páginas sin texto; renderizado con pypdfium2; confianza media registrada; todo dato derivado queda `needs_review`.
- **GROBID local opcional**: solo URLs de loopback salvo autorización explícita de procesamiento externo; consolidación externa desactivada; los valores se localizan en el texto extraído y, si no se encuentran, quedan `needs_review` sin evidencia inventada.
- **Ficha maestra** (`master_record_fields`, 80 campos en 5 grupos: bibliografía, contexto, diseño y métodos, evaluación y resultados, ética y reproducibilidad). Las reglas `rules-v1` proponen 36 campos; el resto se completa manualmente hasta la Fase 4 (IA).
- **Evidencia verificable**: cada propuesta guarda cita literal (subcadena exacta del texto de página), página, bounding box, sección, hash SHA-256 del documento, ejecución, extractor, versión de reglas y método. Varias evidencias por valor.
- **Estados diferenciados**: `not_found` (el extractor buscó y no encontró) frente a `not_reported` (una persona lo afirma). Una restricción `CHECK` en SQLite impide que un actor no humano declare `not_reported`.
- **Contradicciones**: campos de valor único con varios valores activos distintos pasan a `needs_review` y se marcan en conflicto.
- **Revisión humana**: confirmar, rechazar, editar, “No reportado”, “No aplica”, alta manual con cita (verificada contra el texto de la página), historial inmutable de revisiones y restauración de cualquier revisión; cada cambio genera evento de auditoría.
- **Procesamiento incremental**: caché por artículo + SHA-256 del PDF + huella de opciones (versión del worker, reglas, OCR, GROBID). Re-extraer sustituye propuestas no tocadas y conserva valores revisados.
- **Estructura**: secciones navegables, tablas (pdfplumber) y figuras por leyenda, referencias separadas con DOI y enlace a artículos de la biblioteca con el mismo DOI.
- **Interfaz**: pestañas Ficha / Estructura / Metadatos; visor que salta a la página y resalta la caja de evidencia; página **Tareas**; página **Evidencia** con búsqueda y enlaces directos; panel de diagnóstico y preparación del entorno en Configuración; contadores reales en Inicio.

## Criterios de aceptación (backlog) y verificación

| Criterio | Verificación |
|---|---|
| JSON tipado | Tipos Rust (`serde`), TypeScript y Python; prueba `rust_constants_match_the_python_worker` |
| Timeout / cancelación / progreso | `timeout_kills_the_worker`, `running_jobs_can_be_cancelled_paused_and_resumed`, flujo `hello → progress → result` |
| Texto por página | `real_worker_extracts_traceable_evidence` comprueba `text_source = pdf_text` |
| Ficha editable | `human_review_creates_revisions_and_can_revert` |
| Cada propuesta enlaza evidencia | `test_every_quote_is_literal_page_text_with_page_and_bbox` y la prueba de integración real contra SQLite |
| Reinicio recupera trabajos | `interrupted_jobs_are_recovered_after_restart` |

## Migración

- `0005_scientific_extraction.sql`: ajustes de extracción en `app_settings`; columnas de cola en `ingestion_jobs`; metadatos de ejecución en `extraction_runs`; origen de texto y OCR en `document_pages`; enlaces a ejecución en bloques, tablas/figuras y referencias; tablas `master_record_fields`, `master_record_values`, `master_record_value_revisions`, `master_value_evidence`; índices.

La migración es aditiva: las bases de Fase 2 se actualizan sin pérdida.

## Pruebas ejecutadas (28 oct 2026, MacBook Pro Apple Silicon)

```text
python/.venv (Python 3.15, pdfplumber 0.11.10, Tesseract 5.5.3):
  PYTHONPATH=. .venv/bin/python -B -m unittest discover -s tests -p 'test_*.py'
  Ran 21 tests — OK

/usr/bin/python3 (3.9.6, sin pdfplumber): Ran 21 tests — OK (skipped=11)

cd src-tauri && cargo test           → 22 passed; 0 failed
cd src-tauri && cargo clippy --all-targets → 0 avisos
npm run test                          → 9 passed (4 archivos)
npm run build                         → OK
```

Casos cubiertos: PDF nativo de una y dos columnas, página escaneada con OCR real, OCR desactivado, PDF falso, hash alterado, versión de protocolo incompatible, modo sin pdfplumber, GROBID local simulado y bloqueo de GROBID remoto, valores en conflicto, caída del worker con reintento, error tipado sin reintento, timeout, pausa/reanudación/cancelación, reinicio con trabajos pendientes, caché, re-extracción que preserva revisiones, campos desconocidos descartados.

## Cómo ejecutar y comprobar

```bash
# Entorno de desarrollo del extractor (una vez)
cd python && python3 -m venv .venv && .venv/bin/pip install -r requirements.txt && cd ..
# OCR opcional
brew install tesseract tesseract-lang

npm run tauri dev
```

1. Configuración → Extracción científica → **Comprobar**: debe mostrar Python, pdfplumber y Tesseract.
2. Biblioteca → Importar PDF: la tarea aparece en **Tareas** con progreso real.
3. Abrir el artículo → **Ficha científica**: propuestas con cita; «Ver en página N» salta y resalta la evidencia.
4. Confirmar, editar y restaurar desde **Historial**; re-extraer y verificar que los valores revisados se conservan.
5. Cerrar la app durante una extracción y reabrir: la tarea vuelve a la cola.

En la app empaquetada (sin `python/.venv`) se usa «Preparar entorno del extractor», que crea un entorno virtual en la carpeta de datos y descarga `pdfplumber==0.11.10` tras confirmación explícita. Sin ese paso la extracción funciona en **modo texto básico** (sin coordenadas, tablas ni OCR), con aviso visible.

## Checklist de regresión

- [x] Importación PDF/RIS/BibTeX/CSV, duplicados, colecciones, FTS5 y edición de metadatos (pruebas de Fase 2 en verde).
- [x] La importación sigue siendo atómica; la extracción corre después, en segundo plano.
- [x] Migraciones 0001–0004 intactas; 0005 aditiva.
- [x] El visor PDF sigue cargando solo desde `$APPDATA/library/**`.
- [x] Modo solo local: GROBID remoto rechazado al guardar la configuración y en el worker.

## Riesgos y limitaciones declaradas

- Las reglas son deterministas y conservadoras: cobertura parcial de la ficha; los campos narrativos (problema, motivación, marco teórico, trabajos relacionados) se completarán con IA local en Fase 4, siempre como propuestas.
- El orden de lectura cubre una y dos columnas; maquetaciones de tres columnas, texto rotado y notas laterales pueden desordenarse.
- Las tablas sin líneas de cuadrícula se registran solo por su leyenda; fórmulas y gráficos no se interpretan.
- La pausa de una extracción en curso la reinicia desde el principio al reanudar (no hay puntos de control intra-documento).
- La caja de evidencia es la de las líneas que contienen la cita; no se resalta a nivel de carácter.
- El visor no tiene todavía capa de texto seleccionable ni anotaciones.
- `/usr/bin/python3` solo se usa si existen las Command Line Tools, para no abrir el instalador de macOS.
- Firma y notarización siguen sin realizarse.

## Siguiente fase

Fase 4 — Ollama y RAG local: descubrimiento de modelos, embeddings separados del LLM, índice FTS5 por página + vectorial, consultas con citas navegables reutilizando `evidence_spans`, y propuestas de IA para los campos narrativos con el mismo flujo de revisión.
