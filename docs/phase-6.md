# Cierre de Fase 6 — Metodología de revisión sistemática

Cada hoja del formato de evidencia (`Leer`, `Control`, `RQs`, `Cadena`, `I_E`, `Calidad`, `Extraccion`) es ahora un módulo interactivo dentro de cada revisión (`/revisiones/:id`), con persistencia en SQLite, validación en el núcleo Rust y auditoría por proyecto.

## Módulos implementados

| Hoja | Módulo | Qué hace |
|---|---|---|
| — | Resumen | Tipo, estado, modo de revisión (uno o dos revisores), umbral de calidad, regla de faltantes; revisores con rol (responsable, revisor, tercer revisor); revisor activo; corpus desde la biblioteca; conteo por etapa |
| Leer | Guía | Pasos, diferencias SLR / mapeo / alcance, advertencias (PRISMA es guía de reporte) y referencias (Petersen et al. 2015, Kitchenham y Charters 2007, Page et al. 2021, Tricco et al. 2018). Contenido propio; no se redistribuye la plantilla |
| (protocolo) | Protocolo | Borrador editable por secciones; aprobar congela una instantánea de RQs, aspectos, criterios, calidad y cadenas; tras aprobar, solo cambia con enmienda motivada que crea una nueva versión |
| Control | Artículos de control | Estudios clave con justificación; cobertura calculada: un artículo cuenta como recuperado solo si aparece en los resultados importados de alguna cadena (indica sensibilidad, no exhaustividad); aviso si hay menos de cinco |
| RQs | Preguntas y aspectos | RQs ilimitadas con código, categoría, objetivo y subpreguntas; aspectos tipados (texto, número, sí/no, fecha, categoría con taxonomía, lista) con unidad, instrucciones, ejemplos, obligatorio y vínculo opcional con un campo de la ficha maestra; bloque General |
| Cadena | Cadenas de búsqueda | Base de datos, fecha, sintaxis original, cantidad reportada, idioma, periodo, filtros; versiones; importación del archivo exportado (RIS/BibTeX/CSV) enlazada a la cadena, con registros originales, duplicados por DOI sin fusión y alta en el corpus; aviso si lo importado no coincide con la cantidad reportada. No hay scraping de bases con licencia |
| I_E | Cribado | Criterios de inclusión/exclusión por proyecto; fase de metadatos (título, resumen, palabras clave, año, autores, venue) y de texto completo; marcas ✓/✗/? por criterio; decisiones `include`, `exclude`, `maybe`, `awaiting_full_text` con criterio, justificación, revisor y fecha; exclusiones en texto completo exigen el criterio (motivo PRISMA); estudios secundarios para trabajos relacionados |
| — | Doble revisión | Cada revisor decide de forma independiente; el acuerdo finaliza automáticamente y el desacuerdo abre un conflicto que resuelve un tercero con motivo; kappa de Cohen con advertencia de muestra pequeña |
| Calidad | Evaluación | Preguntas Q1…Qn con escala configurable (etiqueta=valor), peso y descripción; valida que cada puntaje pertenezca a la escala; total = Σ peso × valor frente al máximo de la escala; faltantes como 0 o fuera del máximo; umbral en %; consenso o conflicto en doble revisión |
| Extraccion | Matriz | Filas: estudios incluidos (opcionalmente elegibles), con `Id`, `Reference`, `Title`, `Year`; columnas: General y cada RQ con sus aspectos (grupos según las RQs reales, sin repetir etiquetas); varias filas de valor por celda; estados de verificación; cita y página (verificadas contra el texto de la página); prellenado desde valores **confirmados** de la ficha maestra como propuestas que conservan su evidencia |
| — | Conflictos y auditoría | Lista de conflictos abiertos y resueltos; diario de decisiones con actor, acción y valores antes/después |

Las etapas del corpus (`identificado → en cribado → elegible → incluido/excluido`) se derivan exclusivamente de las decisiones finales registradas.

## Migración

- `0008_review_methodology.sql`: ajustes de proyecto (modo, umbral, faltantes, revisor activo); corpus con Id secuencial, marca de secundario y retiro reversible; RQs con categoría y subpreguntas; aspectos en `extraction_fields` (unidad, taxonomía, instrucciones, ejemplos, vínculo con la ficha); `rq_aspects` eliminada (nunca tuvo datos); cadenas archivables e importaciones ligadas a su lote; marcas por criterio en decisiones; escala y descripción de calidad; consenso; detalles de conflictos; índices.

## Pruebas

```text
cargo test               → 48 passed (5 de flujo completo de la Fase 6 + 2 unitarias de la matriz)
cargo clippy --all-targets → 0 avisos
npm run test             → 18 passed
python unittest          → 21 OK
```

Las pruebas recorren: protocolo aprobado con instantánea y enmienda obligatoria; importación de cadena con duplicado por DOI, corpus con Id 1..n, reimportación idempotente y cobertura de control; cribado simple (exclusión sin motivo rechazada, criterio obligatorio en texto completo) y doble (conflicto, resolución, acuerdo automático, kappa); calidad (puntaje fuera de escala rechazado, totales, conflicto, consenso con faltante fuera del máximo, umbral); matriz (solo incluidos, taxonomía validada, estado confirmado sin valor rechazado, evidencia literal, prellenado no destructivo desde la ficha) y auditoría.

## Limitaciones

- La colaboración es local: las decisiones de otro revisor se registran seleccionándolo como «revisor activo». La sincronización e intercambio entre revisores es la Fase 9.
- La IA aún no sugiere decisiones de cribado ni valores de la matriz del proyecto; la ficha maestra (Fases 3–5) es la vía de propuestas con evidencia.
- No hay una política de IA externa por proyecto; la protección por documento es la marca de confidencialidad (Fase 5).
- La exportación al libro de siete hojas y la versión ampliada con procedencia son la Fase 7; el diagrama PRISMA, la Fase 8.
