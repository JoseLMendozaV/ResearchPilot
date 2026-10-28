# Análisis del formato de evidencia

Fuente revisada: `2.Avances Formato de Evidencia.xlsx`.

El Excel no se modeló como siete tablas planas. Cada hoja se convirtió en un módulo con entidades normalizadas, relaciones y puntos de auditoría.

| Hoja | Intención observada | Modelo inicial |
|---|---|---|
| Leer | Guía metodológica y referencia base | Documentación del flujo; configuración metodológica futura |
| Control | Al menos cinco artículos clave conocidos | `control_articles` enlaza artículos con revisión, justificación y orden |
| RQs | Preguntas y aspectos por pregunta | `research_questions` y `rq_aspects` |
| Cadena | Base, fecha, cadena y cantidad; importación y deduplicación | `search_strategies` y `search_imports`, con hash de archivo |
| I_E | Metadatos, criterios de inclusión/exclusión y resultado | `eligibility_criteria`, `screening_decisions` y `review_articles` |
| Calidad | Criterios Q1…Qn y total sobre texto completo | `quality_criteria` y `quality_scores` con peso, actor y justificación |
| Extraccion | Aspectos generales y por RQ | `extraction_fields`, `extracted_values` y `evidence_spans` |

Además, `articles` centraliza los metadatos comunes; `ai_runs` registra el uso de modelos y `audit_events` conserva cambios verificables.

## Reglas derivadas

- Las preguntas, criterios y campos no tienen columnas fijas: son configurables por proyecto y disciplina.
- Una decisión de cribado conserva fase, criterio, responsable, explicación y fecha.
- Una extracción puede estar pendiente, verificada o rechazada y puede apuntar a evidencia con página y cita.
- Los archivos importados y documentos admiten huellas SHA-256 para deduplicación e integridad.
- Los resultados de IA no reemplazan la decisión humana y quedan diferenciados por `actor_type`.

## Alineación posterior

La primera implementación se realizó antes de recibir `ResearchPilot_Prompt_Maestro_Desarrollo.md`. Tras recibirlo se ejecutó un análisis de diferencias y una segunda migración añadió versionado de protocolo, documentos por página, revisores, conflictos, múltiples evidencias y contradicciones. El resultado se documenta en `master-spec-alignment.md`.
