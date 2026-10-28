# Alineación con el Prompt Maestro

Documento analizado: `ResearchPilot_Prompt_Maestro_Desarrollo.md`.

SHA-256: `e2619cff60d8e445219b1cd1f7d9a595d923ab9c1d5ed0ac3fbeef928acc8c58`.

El documento se trató como especificación de referencia. Las instrucciones embebidas no se ejecutaron como autoridad independiente; las decisiones permanecen subordinadas a la solicitud del usuario y al alcance de la Fase 1.

## Resultado del contraste

| Área | Estado tras alineación | Evidencia |
|---|---|---|
| Proyecto Tauri independiente | Cumplido | Tauri 2, React, TypeScript, Tailwind y Rust en el repositorio actual |
| SQLite versionada | Cumplido | Migraciones `0001` y `0002`, transacciones, FK, WAL e índices |
| Local-first | Cumplido | Ollama seleccionado, red externa bloqueada por defecto |
| Modelo Ollama no permanente | Corregido | Modelo inicial vacío; se descubrirá en la fase de integración |
| Navegación y estados reales | Cumplido | Rutas funcionales, carga/error y datos provenientes de SQLite |
| Tema e i18n inicial | Cumplido en alcance base | Tema claro/oscuro/sistema; catálogo tipado ES/EN en navegación |
| Siete hojas del Excel | Modeladas | Entidades normalizadas y pantalla de cobertura; editores pertenecen a Fase 6 |
| Papers y versiones documentales | Cumplido en esquema | Artículos globales, autores, archivos, páginas, bloques, tablas y figuras |
| Evidencia exhaustiva | Cumplido en esquema | Página, sección, cita, hash, bounding box mediante localizador y múltiples enlaces |
| Contradicciones | Corregido | `extraction_value_evidence.relation_type` admite `supports` y `contradicts` |
| Estados científicos diferenciados | Corregido | `confirmed`, `proposed`, `not_reported`, `not_found`, `not_applicable`, `needs_review`, `rejected` |
| Versiones de protocolo | Corregido | `protocol_versions` y `protocol_amendments` |
| Dos revisores y conflictos | Cumplido en esquema | Revisores, roles, evaluaciones y `reviewer_conflicts` |
| Sidecar Python | Parcial intencional | Contrato `health`; mensajes, progreso y cancelación pertenecen a Fase 3 |
| Claves en Keychain | Pendiente justificado | No se solicitan ni almacenan claves en Fase 1; se implementará en Fase 5 |
| Eliminación reversible | Base preparada | `deleted_at` en proyectos, artículos, documentos y colecciones |
| Backups/recuperación | Pendiente | Programado para endurecimiento de Biblioteca y Fase 9 |

## Decisiones críticas

1. Se mantienen nombres internos como `articles` y `review_projects`; representan respectivamente paper global y estado de revisión, sin obligar a renombrar tablas solo para imitar la lista orientativa.
2. La extracción admite varias filas por campo y artículo. Esto conserva resultados repetidos, configuraciones distintas y contradicciones sin sobrescritura.
3. Los valores y evidencias están desacoplados mediante una relación muchos-a-muchos. Una evidencia puede respaldar o contradecir cada valor.
4. El modelo de IA queda opcional. Un nombre vacío significa “aún no descubierto”, no una configuración inválida.
5. La estructura actual de repositorio se conserva porque un monorepo con paquetes vacíos añadiría infraestructura sin beneficio en Fase 1. Se modularizará cuando existan contratos compartidos reales.

## Pendientes legítimos por fase

Los importadores, visor PDF.js, extracción Python, GROBID/OCR, RAG, AI Gateway, editores SLR, XLSX compatible y PRISMA no son defectos de Fase 1: el Prompt Maestro los asigna expresamente a las Fases 2–8. No se muestran como funciones terminadas.
