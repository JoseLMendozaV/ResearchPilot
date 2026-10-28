-- Fase 3: extracción científica con evidencia, cola persistente y ficha maestra.

ALTER TABLE app_settings ADD COLUMN auto_extract_on_import INTEGER NOT NULL DEFAULT 1 CHECK (auto_extract_on_import IN (0, 1));
ALTER TABLE app_settings ADD COLUMN ocr_enabled INTEGER NOT NULL DEFAULT 1 CHECK (ocr_enabled IN (0, 1));
ALTER TABLE app_settings ADD COLUMN ocr_languages TEXT NOT NULL DEFAULT 'eng';
ALTER TABLE app_settings ADD COLUMN grobid_enabled INTEGER NOT NULL DEFAULT 0 CHECK (grobid_enabled IN (0, 1));
ALTER TABLE app_settings ADD COLUMN grobid_url TEXT NOT NULL DEFAULT 'http://127.0.0.1:8070';
ALTER TABLE app_settings ADD COLUMN extraction_timeout_seconds INTEGER NOT NULL DEFAULT 600 CHECK (extraction_timeout_seconds BETWEEN 30 AND 7200);
ALTER TABLE app_settings ADD COLUMN extractor_python_path TEXT NOT NULL DEFAULT '';

ALTER TABLE ingestion_jobs ADD COLUMN document_file_id TEXT REFERENCES document_files(id) ON DELETE SET NULL;
ALTER TABLE ingestion_jobs ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0);
ALTER TABLE ingestion_jobs ADD COLUMN max_attempts INTEGER NOT NULL DEFAULT 2 CHECK (max_attempts > 0);
ALTER TABLE ingestion_jobs ADD COLUMN stage TEXT;
ALTER TABLE ingestion_jobs ADD COLUMN message TEXT;
ALTER TABLE ingestion_jobs ADD COLUMN started_at TEXT;
ALTER TABLE ingestion_jobs ADD COLUMN finished_at TEXT;

ALTER TABLE extraction_runs ADD COLUMN job_id TEXT REFERENCES ingestion_jobs(id) ON DELETE SET NULL;
ALTER TABLE extraction_runs ADD COLUMN document_file_id TEXT REFERENCES document_files(id) ON DELETE SET NULL;
ALTER TABLE extraction_runs ADD COLUMN extractor_version TEXT;
ALTER TABLE extraction_runs ADD COLUMN engine TEXT;
ALTER TABLE extraction_runs ADD COLUMN options_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE extraction_runs ADD COLUMN options_sha256 TEXT;
ALTER TABLE extraction_runs ADD COLUMN stats_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE extraction_runs ADD COLUMN warnings_json TEXT NOT NULL DEFAULT '[]';

-- Texto por página: origen (texto PDF básico, diseño, OCR) y confianza del OCR.
ALTER TABLE document_pages ADD COLUMN text_source TEXT NOT NULL DEFAULT 'pdf_text_basic';
ALTER TABLE document_pages ADD COLUMN ocr_confidence REAL CHECK (ocr_confidence IS NULL OR (ocr_confidence >= 0 AND ocr_confidence <= 1));
ALTER TABLE document_pages ADD COLUMN column_count INTEGER;
ALTER TABLE document_pages ADD COLUMN extraction_run_id TEXT REFERENCES extraction_runs(id) ON DELETE SET NULL;

ALTER TABLE document_blocks ADD COLUMN extraction_run_id TEXT REFERENCES extraction_runs(id) ON DELETE CASCADE;
ALTER TABLE document_blocks ADD COLUMN font_size REAL;
ALTER TABLE document_blocks ADD COLUMN canonical_section TEXT;

ALTER TABLE tables_figures ADD COLUMN extraction_run_id TEXT REFERENCES extraction_runs(id) ON DELETE CASCADE;
ALTER TABLE tables_figures ADD COLUMN page_number INTEGER;
ALTER TABLE tables_figures ADD COLUMN caption_bbox_json TEXT;

ALTER TABLE bibliographic_references ADD COLUMN extraction_run_id TEXT REFERENCES extraction_runs(id) ON DELETE CASCADE;
ALTER TABLE bibliographic_references ADD COLUMN page_number INTEGER;

-- Catálogo de la ficha maestra: global por artículo, independiente de cada revisión.
CREATE TABLE master_record_fields (
    field_key TEXT PRIMARY KEY,
    group_key TEXT NOT NULL CHECK (group_key IN ('bibliography', 'context', 'design_methods', 'evaluation_results', 'ethics_reproducibility')),
    label_es TEXT NOT NULL,
    label_en TEXT NOT NULL,
    data_type TEXT NOT NULL CHECK (data_type IN ('text', 'number', 'list', 'url', 'date', 'metric')),
    multi_valued INTEGER NOT NULL DEFAULT 0 CHECK (multi_valued IN (0, 1)),
    description TEXT NOT NULL DEFAULT '',
    display_order INTEGER NOT NULL
);

CREATE TABLE master_record_values (
    id TEXT PRIMARY KEY,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    field_key TEXT NOT NULL REFERENCES master_record_fields(field_key),
    value_json TEXT NOT NULL DEFAULT 'null',
    display_value TEXT NOT NULL DEFAULT '',
    value_type TEXT NOT NULL,
    unit TEXT,
    raw_quote TEXT,
    section TEXT,
    page INTEGER CHECK (page IS NULL OR page > 0),
    bbox_json TEXT,
    document_file_id TEXT REFERENCES document_files(id) ON DELETE SET NULL,
    source_document_hash TEXT,
    extraction_run_id TEXT REFERENCES extraction_runs(id) ON DELETE SET NULL,
    provider TEXT,
    model TEXT,
    prompt_version TEXT,
    method TEXT,
    origin TEXT NOT NULL CHECK (origin IN ('rules', 'ocr', 'grobid', 'ai', 'human', 'import')),
    status TEXT NOT NULL CHECK (status IN ('confirmed', 'proposed', 'not_reported', 'not_found', 'not_applicable', 'needs_review', 'rejected')),
    reviewer_id TEXT REFERENCES reviewers(id) ON DELETE SET NULL,
    reviewed_at TEXT,
    notes TEXT NOT NULL DEFAULT '',
    revision_number INTEGER NOT NULL DEFAULT 1 CHECK (revision_number > 0),
    superseded_by_run_id TEXT REFERENCES extraction_runs(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    -- Solo una persona puede afirmar que el artículo no reporta un dato.
    CHECK (status <> 'not_reported' OR origin = 'human' OR reviewer_id IS NOT NULL)
);

-- Historial inmutable: cada cambio crea una revisión; revertir crea otra.
CREATE TABLE master_record_value_revisions (
    id TEXT PRIMARY KEY,
    value_id TEXT NOT NULL REFERENCES master_record_values(id) ON DELETE CASCADE,
    revision_number INTEGER NOT NULL CHECK (revision_number > 0),
    value_json TEXT NOT NULL,
    display_value TEXT NOT NULL,
    status TEXT NOT NULL,
    notes TEXT NOT NULL DEFAULT '',
    actor_type TEXT NOT NULL CHECK (actor_type IN ('human', 'ai', 'system', 'import')),
    actor_name TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    UNIQUE (value_id, revision_number)
);

CREATE TABLE master_value_evidence (
    value_id TEXT NOT NULL REFERENCES master_record_values(id) ON DELETE CASCADE,
    evidence_id TEXT NOT NULL REFERENCES evidence_spans(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL DEFAULT 'supports' CHECK (relation_type IN ('supports', 'contradicts')),
    PRIMARY KEY (value_id, evidence_id)
);

CREATE INDEX idx_master_values_article ON master_record_values(article_id, field_key, superseded_by_run_id);
CREATE INDEX idx_master_values_status ON master_record_values(status, updated_at);
CREATE INDEX idx_master_revisions_value ON master_record_value_revisions(value_id, revision_number);
CREATE INDEX idx_master_value_evidence_evidence ON master_value_evidence(evidence_id);
CREATE INDEX idx_ingestion_jobs_article ON ingestion_jobs(article_id, job_type, created_at);
CREATE INDEX idx_extraction_runs_cache ON extraction_runs(article_id, source_document_hash, options_sha256, status);
CREATE INDEX idx_tables_figures_article ON tables_figures(article_id, page_number);
CREATE INDEX idx_references_article ON bibliographic_references(citing_article_id, ordinal);
CREATE INDEX idx_evidence_created ON evidence_spans(created_at);

INSERT INTO master_record_fields (field_key, group_key, label_es, label_en, data_type, multi_valued, display_order) VALUES
('bibliography.title', 'bibliography', 'Título', 'Title', 'text', 0, 10),
('bibliography.translated_title', 'bibliography', 'Título traducido', 'Translated title', 'text', 0, 20),
('bibliography.doi', 'bibliography', 'DOI', 'DOI', 'text', 0, 30),
('bibliography.url', 'bibliography', 'URL', 'URL', 'url', 0, 40),
('bibliography.isbn', 'bibliography', 'ISBN', 'ISBN', 'text', 1, 50),
('bibliography.issn', 'bibliography', 'ISSN', 'ISSN', 'text', 1, 60),
('bibliography.publication_type', 'bibliography', 'Tipo de publicación', 'Publication type', 'text', 0, 70),
('bibliography.year', 'bibliography', 'Año', 'Year', 'number', 0, 80),
('bibliography.publication_date', 'bibliography', 'Fecha de publicación', 'Publication date', 'date', 0, 90),
('bibliography.venue', 'bibliography', 'Revista o conferencia', 'Venue', 'text', 0, 100),
('bibliography.volume', 'bibliography', 'Volumen', 'Volume', 'text', 0, 110),
('bibliography.issue', 'bibliography', 'Número', 'Issue', 'text', 0, 120),
('bibliography.pages', 'bibliography', 'Páginas', 'Pages', 'text', 0, 130),
('bibliography.publisher', 'bibliography', 'Editorial', 'Publisher', 'text', 0, 140),
('bibliography.authors', 'bibliography', 'Autores', 'Authors', 'list', 0, 150),
('bibliography.orcid', 'bibliography', 'ORCID', 'ORCID', 'list', 0, 160),
('bibliography.affiliations', 'bibliography', 'Afiliaciones', 'Affiliations', 'list', 0, 170),
('bibliography.countries', 'bibliography', 'Países', 'Countries', 'list', 0, 180),
('bibliography.language', 'bibliography', 'Idioma', 'Language', 'text', 0, 190),
('bibliography.abstract', 'bibliography', 'Resumen', 'Abstract', 'text', 0, 200),
('bibliography.keywords', 'bibliography', 'Palabras clave', 'Keywords', 'list', 0, 210),
('bibliography.cited_references_count', 'bibliography', 'Referencias citadas', 'Cited references', 'number', 0, 220),
('context.discipline', 'context', 'Disciplina', 'Discipline', 'text', 0, 300),
('context.problem', 'context', 'Problema', 'Problem', 'text', 1, 310),
('context.background', 'context', 'Antecedentes', 'Background', 'text', 1, 320),
('context.motivation', 'context', 'Motivación', 'Motivation', 'text', 1, 330),
('context.justification', 'context', 'Justificación', 'Justification', 'text', 1, 340),
('context.objectives', 'context', 'Objetivos', 'Objectives', 'text', 1, 350),
('context.research_questions', 'context', 'Preguntas de investigación', 'Research questions', 'text', 1, 360),
('context.hypotheses', 'context', 'Hipótesis', 'Hypotheses', 'text', 1, 370),
('context.contributions', 'context', 'Contribuciones', 'Contributions', 'text', 1, 380),
('context.main_claims', 'context', 'Afirmaciones principales', 'Main claims', 'text', 1, 390),
('context.theoretical_framework', 'context', 'Marco teórico', 'Theoretical framework', 'text', 1, 400),
('context.related_work', 'context', 'Trabajos relacionados', 'Related work', 'text', 1, 410),
('context.stated_gaps', 'context', 'Brechas declaradas', 'Stated gaps', 'text', 1, 420),
('design_methods.approach', 'design_methods', 'Enfoque', 'Approach', 'text', 0, 500),
('design_methods.study_design', 'design_methods', 'Diseño del estudio', 'Study design', 'text', 0, 510),
('design_methods.unit_of_analysis', 'design_methods', 'Unidad de análisis', 'Unit of analysis', 'text', 0, 520),
('design_methods.population', 'design_methods', 'Población', 'Population', 'text', 1, 530),
('design_methods.sample', 'design_methods', 'Muestra', 'Sample', 'text', 1, 540),
('design_methods.sample_size', 'design_methods', 'Tamaño de muestra', 'Sample size', 'number', 1, 550),
('design_methods.sampling_criteria', 'design_methods', 'Criterios de muestreo', 'Sampling criteria', 'text', 1, 560),
('design_methods.instruments', 'design_methods', 'Instrumentos', 'Instruments', 'text', 1, 570),
('design_methods.variables', 'design_methods', 'Variables', 'Variables', 'text', 1, 580),
('design_methods.datasets', 'design_methods', 'Datasets', 'Datasets', 'text', 1, 590),
('design_methods.data_sources_licenses', 'design_methods', 'Orígenes y licencias de datos', 'Data sources and licenses', 'text', 1, 600),
('design_methods.data_size', 'design_methods', 'Tamaño de los datos', 'Data size', 'text', 1, 610),
('design_methods.annotation', 'design_methods', 'Anotación', 'Annotation', 'text', 1, 620),
('design_methods.data_split', 'design_methods', 'División train/validation/test', 'Data split', 'text', 1, 630),
('design_methods.preprocessing', 'design_methods', 'Preprocesamiento', 'Preprocessing', 'text', 1, 640),
('design_methods.algorithms', 'design_methods', 'Algoritmos', 'Algorithms', 'text', 1, 650),
('design_methods.architectures', 'design_methods', 'Arquitecturas', 'Architectures', 'text', 1, 660),
('design_methods.base_models', 'design_methods', 'Modelos base', 'Base models', 'text', 1, 670),
('design_methods.baselines', 'design_methods', 'Comparadores / baselines', 'Baselines', 'text', 1, 680),
('design_methods.hyperparameters', 'design_methods', 'Hiperparámetros', 'Hyperparameters', 'text', 1, 690),
('design_methods.training', 'design_methods', 'Entrenamiento', 'Training', 'text', 1, 700),
('design_methods.validation', 'design_methods', 'Validación', 'Validation', 'text', 1, 710),
('design_methods.testing', 'design_methods', 'Pruebas', 'Testing', 'text', 1, 720),
('design_methods.hardware', 'design_methods', 'Hardware', 'Hardware', 'text', 1, 730),
('design_methods.software', 'design_methods', 'Software', 'Software', 'text', 1, 740),
('design_methods.software_versions', 'design_methods', 'Versiones de software', 'Software versions', 'text', 1, 750),
('design_methods.statistical_methods', 'design_methods', 'Métodos estadísticos', 'Statistical methods', 'text', 1, 760),
('evaluation_results.metrics', 'evaluation_results', 'Métricas', 'Metrics', 'metric', 1, 800),
('evaluation_results.results_text', 'evaluation_results', 'Resultados textuales', 'Reported results', 'text', 1, 810),
('evaluation_results.significance', 'evaluation_results', 'Significancia', 'Significance', 'text', 1, 820),
('evaluation_results.comparisons', 'evaluation_results', 'Comparaciones', 'Comparisons', 'text', 1, 830),
('evaluation_results.ablation', 'evaluation_results', 'Estudios de ablación', 'Ablation studies', 'text', 1, 840),
('evaluation_results.negative_findings', 'evaluation_results', 'Hallazgos negativos', 'Negative findings', 'text', 1, 850),
('evaluation_results.conclusions', 'evaluation_results', 'Conclusiones', 'Conclusions', 'text', 1, 860),
('evaluation_results.threats_to_validity', 'evaluation_results', 'Amenazas a la validez', 'Threats to validity', 'text', 1, 870),
('evaluation_results.limitations', 'evaluation_results', 'Limitaciones', 'Limitations', 'text', 1, 880),
('evaluation_results.future_work', 'evaluation_results', 'Trabajo futuro', 'Future work', 'text', 1, 890),
('ethics_reproducibility.ethics_approval', 'ethics_reproducibility', 'Aprobación ética', 'Ethics approval', 'text', 1, 900),
('ethics_reproducibility.funding', 'ethics_reproducibility', 'Financiamiento', 'Funding', 'text', 1, 910),
('ethics_reproducibility.conflicts_of_interest', 'ethics_reproducibility', 'Conflictos de interés', 'Conflicts of interest', 'text', 1, 920),
('ethics_reproducibility.data_availability', 'ethics_reproducibility', 'Disponibilidad de datos', 'Data availability', 'text', 1, 930),
('ethics_reproducibility.code_availability', 'ethics_reproducibility', 'Disponibilidad de código', 'Code availability', 'text', 1, 940),
('ethics_reproducibility.model_availability', 'ethics_reproducibility', 'Disponibilidad de modelos', 'Model availability', 'text', 1, 950),
('ethics_reproducibility.supplementary_material', 'ethics_reproducibility', 'Material suplementario', 'Supplementary material', 'text', 1, 960),
('ethics_reproducibility.repositories', 'ethics_reproducibility', 'Repositorios y enlaces', 'Repositories and links', 'url', 1, 970);
