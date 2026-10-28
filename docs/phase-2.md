# Cierre de Fase 2 — Biblioteca y visor

## Objetivos implementados

- Importación individual o por lotes de hasta 100 PDF mediante selector nativo.
- Validación de extensión, firma `%PDF-`, tamaño máximo de 200 MB y estructura legible.
- Copia original administrada en `$APPDATA/library/<article_id>/original.pdf`.
- SHA-256 para integridad y deduplicación exacta sin volver a copiar.
- Lectura de título, autor, asunto, palabras clave, fecha y cantidad de páginas desde metadatos PDF cuando existen.
- Extracción inicial de texto por página con límite de descompresión; páginas sin texto se marcan `ocr_required`.
- Importación RIS, BibTeX y CSV con lote, hash y registro de procedencia.
- DOI normalizado como duplicado seguro; coincidencias exactas de título/año sin DOI se conservan separadas y se registran como candidatas.
- Biblioteca persistente con búsqueda FTS5 en título, resumen, autores y palabras clave.
- Creación y filtrado de colecciones; asignación desde la ficha del artículo.
- Visor PDF.js local con navegación, zoom y carga restringida a la carpeta administrada.
- Edición de metadatos bibliográficos con validación, actualización FTS5 y auditoría antes/después.
- Archivado reversible: el registro y el PDF se conservan, pero dejan de aparecer en la biblioteca activa.

## Seguridad

- El selector nativo autoriza los archivos elegidos, pero el WebView no recibe acceso general al sistema de archivos.
- El protocolo asset solo permite `$APPDATA/library/**`.
- PDF.js y su worker se empaquetan localmente; no se usa CDN.
- Los archivos no ejecutan instrucciones ni se envían a servicios externos.
- La extracción por página limita el tamaño descomprimido para reducir riesgo de PDFs hostiles.

## Migraciones

- `0003_library_import.sql`: procedencia de archivo e índices de biblioteca.
- `0004_bibliography_import.sql`: lotes, registros originales y candidatos duplicados.

## Pruebas

Las pruebas Rust cubren:

1. PDF válido generado como fixture con licencia propia del proyecto.
2. Extracción de una página y metadatos.
3. Duplicado exacto por SHA-256.
4. Rechazo de un archivo falso con extensión PDF sin mutar SQLite.
5. Edición de metadatos, actualización FTS5 y archivado.
6. Colecciones persistentes.
7. Importación RIS y procedencia.
8. Dos registros ambiguos conservados y marcados como candidatos, sin fusión automática.

React Testing Library verifica la visualización de artículos y las acciones reales de importación.

## Limitaciones declaradas

- OCR local, GROBID, tablas, figuras y ficha científica exhaustiva pertenecen a la Fase 3.
- El visor todavía no incorpora capa de texto seleccionable, resaltados ni anotaciones.
- Los candidatos duplicados quedan registrados, pero la pantalla avanzada de comparación/fusión reversible está pendiente.
- Los parsers RIS/BibTeX/CSV cubren campos bibliográficos comunes; el mapeo visual de columnas y XLSX se implementará en la fase de importación/exportación avanzada.
- No se consultan DOI ni metadatos en línea automáticamente para preservar el comportamiento local-first.
- Progreso persistente, pausa, reanudación y cancelación de extracción corresponden a la cola de trabajos de Fase 3.

## Siguiente fase

Fase 3: sidecar Python tipado, cola persistente, extracción científica estructurada por página, GROBID y OCR opcionales, evidencia editable y recuperación tras reinicio.
