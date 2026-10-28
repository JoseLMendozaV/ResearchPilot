-- Fase 6: metodología SLR por proyecto (Control, RQs, Cadena, I_E, Calidad, Extracción).

ALTER TABLE review_projects ADD COLUMN screening_mode TEXT NOT NULL DEFAULT 'single' CHECK (screening_mode IN ('single', 'double'));
ALTER TABLE review_projects ADD COLUMN quality_threshold REAL;
ALTER TABLE review_projects ADD COLUMN quality_missing_policy TEXT NOT NULL DEFAULT 'zero' CHECK (quality_missing_policy IN ('zero', 'exclude_from_max'));
ALTER TABLE review_projects ADD COLUMN active_reviewer_id TEXT REFERENCES reviewers(id) ON DELETE SET NULL;

-- Corpus del proyecto: Id secuencial (como la columna Id del formato), estudios secundarios.
ALTER TABLE review_articles ADD COLUMN sequence INTEGER;
ALTER TABLE review_articles ADD COLUMN is_secondary INTEGER NOT NULL DEFAULT 0 CHECK (is_secondary IN (0, 1));
ALTER TABLE review_articles ADD COLUMN notes TEXT NOT NULL DEFAULT '';
ALTER TABLE review_articles ADD COLUMN removed_at TEXT;

ALTER TABLE research_questions ADD COLUMN parent_id TEXT REFERENCES research_questions(id) ON DELETE CASCADE;
ALTER TABLE research_questions ADD COLUMN category TEXT NOT NULL DEFAULT '';

-- Los aspectos de cada RQ son campos de extracción (una sola tabla para la matriz).
-- rq_aspects nunca tuvo datos.
DROP TABLE rq_aspects;
ALTER TABLE extraction_fields ADD COLUMN unit TEXT;
ALTER TABLE extraction_fields ADD COLUMN taxonomy_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE extraction_fields ADD COLUMN instructions TEXT NOT NULL DEFAULT '';
ALTER TABLE extraction_fields ADD COLUMN examples_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE extraction_fields ADD COLUMN master_field_key TEXT REFERENCES master_record_fields(field_key);
ALTER TABLE extraction_fields ADD COLUMN deleted_at TEXT;

ALTER TABLE search_strategies ADD COLUMN deleted_at TEXT;
ALTER TABLE search_imports ADD COLUMN import_batch_id TEXT REFERENCES import_batches(id) ON DELETE SET NULL;
ALTER TABLE search_imports ADD COLUMN linked_count INTEGER NOT NULL DEFAULT 0;

-- Marcas por criterio de cada decisión: {criterion_id: "met" | "not_met" | "unclear"}.
ALTER TABLE screening_decisions ADD COLUMN criteria_json TEXT NOT NULL DEFAULT '{}';

ALTER TABLE quality_criteria ADD COLUMN description TEXT NOT NULL DEFAULT '';
ALTER TABLE quality_criteria ADD COLUMN scale_json TEXT NOT NULL DEFAULT '[{"label":"Sí","value":1},{"label":"Parcialmente","value":0.5},{"label":"No","value":0}]';
ALTER TABLE quality_scores ADD COLUMN is_final INTEGER NOT NULL DEFAULT 0 CHECK (is_final IN (0, 1));

ALTER TABLE reviewer_conflicts ADD COLUMN phase TEXT;
ALTER TABLE reviewer_conflicts ADD COLUMN details_json TEXT NOT NULL DEFAULT '{}';

CREATE INDEX idx_review_articles_sequence ON review_articles(project_id, sequence);
CREATE INDEX idx_screening_final ON screening_decisions(project_id, article_id, phase, is_final, decided_at);
CREATE INDEX idx_conflicts_project ON reviewer_conflicts(project_id, status, conflict_type);
CREATE INDEX idx_extraction_fields_project ON extraction_fields(project_id, research_question_id, display_order);
CREATE INDEX idx_extracted_values_field ON extracted_values(field_id, article_id);
CREATE INDEX idx_search_imports_strategy ON search_imports(search_strategy_id);
CREATE INDEX idx_quality_scores_final ON quality_scores(article_id, is_final);
