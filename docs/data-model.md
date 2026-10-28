# Modelo de datos inicial

## ERD resumido

```mermaid
erDiagram
    REVIEW_PROJECTS ||--o{ PROTOCOL_VERSIONS : versiona
    REVIEW_PROJECTS ||--o{ REVIEW_ARTICLES : contiene
    ARTICLES ||--o{ REVIEW_ARTICLES : participa
    ARTICLES ||--o{ DOCUMENT_FILES : posee
    DOCUMENT_FILES ||--o{ DOCUMENT_PAGES : divide
    DOCUMENT_PAGES ||--o{ DOCUMENT_BLOCKS : estructura
    ARTICLES }o--o{ AUTHORS : PAPER_AUTHORS
    REVIEW_PROJECTS ||--o{ RESEARCH_QUESTIONS : define
    RESEARCH_QUESTIONS ||--o{ RQ_ASPECTS : detalla
    REVIEW_PROJECTS ||--o{ EXTRACTION_FIELDS : configura
    EXTRACTION_FIELDS ||--o{ EXTRACTED_VALUES : recibe
    ARTICLES ||--o{ EXTRACTED_VALUES : produce
    ARTICLES ||--o{ EVIDENCE_SPANS : cita
    EXTRACTED_VALUES }o--o{ EVIDENCE_SPANS : EXTRACTION_VALUE_EVIDENCE
    REVIEW_PROJECTS ||--o{ SCREENING_DECISIONS : registra
    REVIEW_PROJECTS ||--o{ QUALITY_CRITERIA : define
    QUALITY_CRITERIA ||--o{ QUALITY_SCORES : evalua
    REVIEWERS ||--o{ SCREENING_DECISIONS : decide
    REVIEWERS ||--o{ REVIEWER_CONFLICTS : resuelve
    REVIEW_PROJECTS ||--o{ AUDIT_EVENTS : audita
```

## Límites de agregado

- **Biblioteca global:** `articles`, autores, documentos, páginas, bloques, referencias, colecciones y etiquetas.
- **Revisión:** proyecto, versiones del protocolo, corpus, RQs, búsquedas, control, criterios, decisiones y evaluaciones.
- **Extracción verificable:** campos, ejecuciones, valores, evidencias y relaciones de apoyo/contradicción.
- **Operación:** trabajos de ingestión, proveedores/ejecuciones IA, índices de embeddings, exportaciones y auditoría.

## Integridad

- UUID de texto como identificadores estables para intercambio futuro.
- Claves foráneas y cascadas controladas.
- DOI y hashes únicos donde una colisión representa el mismo objeto global; versiones PDF se conservan en `document_files`.
- Eliminación lógica para objetos principales; las eliminaciones físicas internas se reservan para mantenimiento explícito.
- Los valores extraídos no son únicos por artículo/campo: pueden coexistir mediciones, configuraciones y contradicciones.
- La evidencia contiene documento, página, sección, cita y localizador JSON para bounding box cuando esté disponible.
