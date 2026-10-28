-- El modelo se descubre en Ollama; ningún nombre queda fijado como permanente.
UPDATE app_settings SET ollama_model = '' WHERE ollama_model = 'qwen3:8b';

ALTER TABLE review_projects ADD COLUMN deleted_at TEXT;
ALTER TABLE articles ADD COLUMN deleted_at TEXT;

CREATE TABLE protocol_versions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    version_number INTEGER NOT NULL CHECK (version_number > 0),
    content_json TEXT NOT NULL DEFAULT '{}',
    change_summary TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'approved', 'superseded')),
    created_by TEXT NOT NULL,
    approved_at TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (project_id, version_number)
);

CREATE TABLE protocol_amendments (
    id TEXT PRIMARY KEY,
    protocol_version_id TEXT NOT NULL REFERENCES protocol_versions(id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    change_json TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE reviewers (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    email TEXT,
    is_local_profile INTEGER NOT NULL DEFAULT 0 CHECK (is_local_profile IN (0, 1)),
    created_at TEXT NOT NULL
);

CREATE TABLE project_reviewers (
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    reviewer_id TEXT NOT NULL REFERENCES reviewers(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'reviewer' CHECK (role IN ('owner', 'reviewer', 'resolver')),
    PRIMARY KEY (project_id, reviewer_id)
);

CREATE TABLE authors (
    id TEXT PRIMARY KEY,
    full_name TEXT NOT NULL,
    orcid TEXT,
    affiliations_json TEXT NOT NULL DEFAULT '[]',
    countries_json TEXT NOT NULL DEFAULT '[]',
    UNIQUE (orcid)
);

CREATE TABLE paper_authors (
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    author_id TEXT NOT NULL REFERENCES authors(id) ON DELETE CASCADE,
    author_order INTEGER NOT NULL CHECK (author_order >= 0),
    is_corresponding INTEGER NOT NULL DEFAULT 0 CHECK (is_corresponding IN (0, 1)),
    PRIMARY KEY (article_id, author_id)
);

CREATE TABLE document_files (
    id TEXT PRIMARY KEY,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    original_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    media_type TEXT NOT NULL DEFAULT 'application/pdf',
    sha256 TEXT NOT NULL,
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    version_label TEXT NOT NULL DEFAULT 'original',
    is_primary INTEGER NOT NULL DEFAULT 1 CHECK (is_primary IN (0, 1)),
    imported_at TEXT NOT NULL,
    deleted_at TEXT,
    UNIQUE (article_id, sha256)
);

CREATE TABLE document_pages (
    id TEXT PRIMARY KEY,
    document_file_id TEXT NOT NULL REFERENCES document_files(id) ON DELETE CASCADE,
    page_number INTEGER NOT NULL CHECK (page_number > 0),
    text_content TEXT NOT NULL DEFAULT '',
    width REAL,
    height REAL,
    extraction_status TEXT NOT NULL DEFAULT 'pending' CHECK (extraction_status IN ('pending', 'extracted', 'ocr_required', 'failed')),
    UNIQUE (document_file_id, page_number)
);

CREATE TABLE document_blocks (
    id TEXT PRIMARY KEY,
    document_page_id TEXT NOT NULL REFERENCES document_pages(id) ON DELETE CASCADE,
    block_type TEXT NOT NULL DEFAULT 'paragraph',
    reading_order INTEGER NOT NULL DEFAULT 0,
    text_content TEXT NOT NULL DEFAULT '',
    bounding_box_json TEXT,
    section_path TEXT
);

CREATE TABLE tables_figures (
    id TEXT PRIMARY KEY,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    document_page_id TEXT REFERENCES document_pages(id) ON DELETE SET NULL,
    object_type TEXT NOT NULL CHECK (object_type IN ('table', 'figure')),
    label TEXT,
    caption TEXT,
    content_json TEXT NOT NULL DEFAULT '{}',
    bounding_box_json TEXT
);

CREATE TABLE bibliographic_references (
    id TEXT PRIMARY KEY,
    citing_article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    ordinal INTEGER,
    raw_reference TEXT NOT NULL,
    title TEXT,
    doi TEXT,
    matched_article_id TEXT REFERENCES articles(id) ON DELETE SET NULL
);

CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE TABLE collection_articles (
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    added_at TEXT NOT NULL,
    PRIMARY KEY (collection_id, article_id)
);

CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
    color TEXT
);

CREATE TABLE article_tags (
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (article_id, tag_id)
);

ALTER TABLE rq_aspects ADD COLUMN data_type TEXT NOT NULL DEFAULT 'text';
ALTER TABLE rq_aspects ADD COLUMN unit TEXT;
ALTER TABLE rq_aspects ADD COLUMN taxonomy_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE rq_aspects ADD COLUMN instructions TEXT NOT NULL DEFAULT '';
ALTER TABLE rq_aspects ADD COLUMN required INTEGER NOT NULL DEFAULT 0 CHECK (required IN (0, 1));
ALTER TABLE rq_aspects ADD COLUMN validation_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE rq_aspects ADD COLUMN examples_json TEXT NOT NULL DEFAULT '[]';

ALTER TABLE search_strategies ADD COLUMN version_number INTEGER NOT NULL DEFAULT 1 CHECK (version_number > 0);
ALTER TABLE search_strategies ADD COLUMN filters_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE search_strategies ADD COLUMN language TEXT;
ALTER TABLE search_strategies ADD COLUMN period_start INTEGER;
ALTER TABLE search_strategies ADD COLUMN period_end INTEGER;
ALTER TABLE control_articles ADD COLUMN expected_relevant INTEGER NOT NULL DEFAULT 1 CHECK (expected_relevant IN (0, 1));

ALTER TABLE screening_decisions RENAME TO screening_decisions_legacy;
DROP INDEX idx_screening_article;
CREATE TABLE screening_decisions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    phase TEXT NOT NULL CHECK (phase IN ('metadata', 'full_text')),
    decision TEXT NOT NULL CHECK (decision IN ('include', 'exclude', 'maybe', 'awaiting_full_text')),
    criterion_id TEXT REFERENCES eligibility_criteria(id) ON DELETE SET NULL,
    evidence_id TEXT REFERENCES evidence_spans(id) ON DELETE SET NULL,
    reviewer_id TEXT REFERENCES reviewers(id) ON DELETE SET NULL,
    actor_type TEXT NOT NULL CHECK (actor_type IN ('human', 'ai')),
    actor_name TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    is_final INTEGER NOT NULL DEFAULT 0 CHECK (is_final IN (0, 1)),
    decided_at TEXT NOT NULL
);
INSERT INTO screening_decisions (
    id, project_id, article_id, phase, decision, criterion_id,
    actor_type, actor_name, reason, decided_at
)
SELECT id, project_id, article_id, phase,
       CASE decision WHEN 'uncertain' THEN 'maybe' ELSE decision END,
       criterion_id, actor_type, actor_name, reason, decided_at
FROM screening_decisions_legacy;
DROP TABLE screening_decisions_legacy;
CREATE INDEX idx_screening_article ON screening_decisions(project_id, article_id, phase);

ALTER TABLE quality_scores ADD COLUMN reviewer_id TEXT REFERENCES reviewers(id) ON DELETE SET NULL;

CREATE TABLE reviewer_conflicts (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    conflict_type TEXT NOT NULL CHECK (conflict_type IN ('screening', 'quality', 'extraction')),
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'resolved')),
    resolution TEXT,
    resolver_id TEXT REFERENCES reviewers(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    resolved_at TEXT
);

CREATE TABLE extraction_runs (
    id TEXT PRIMARY KEY,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    model TEXT,
    prompt_version TEXT NOT NULL,
    source_document_hash TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed', 'cancelled')),
    started_at TEXT NOT NULL,
    completed_at TEXT,
    error_message TEXT
);

ALTER TABLE evidence_spans ADD COLUMN document_file_id TEXT REFERENCES document_files(id) ON DELETE CASCADE;
ALTER TABLE evidence_spans ADD COLUMN section TEXT;

ALTER TABLE extracted_values RENAME TO extracted_values_legacy;
DROP INDEX idx_extracted_values_article;
CREATE TABLE extracted_values (
    id TEXT PRIMARY KEY,
    field_id TEXT NOT NULL REFERENCES extraction_fields(id) ON DELETE CASCADE,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    raw_quote TEXT,
    normalized_value_json TEXT NOT NULL,
    value_type TEXT NOT NULL DEFAULT 'text',
    unit TEXT,
    section TEXT,
    page INTEGER CHECK (page IS NULL OR page > 0),
    source_document_hash TEXT,
    extraction_run_id TEXT REFERENCES extraction_runs(id) ON DELETE SET NULL,
    provider TEXT,
    model TEXT,
    prompt_version TEXT,
    confidence REAL CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
    status TEXT NOT NULL DEFAULT 'needs_review' CHECK (status IN ('confirmed', 'proposed', 'not_reported', 'not_found', 'not_applicable', 'needs_review', 'rejected')),
    reviewer_id TEXT REFERENCES reviewers(id) ON DELETE SET NULL,
    notes TEXT NOT NULL DEFAULT '',
    actor_type TEXT NOT NULL CHECK (actor_type IN ('human', 'ai', 'import')),
    actor_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
INSERT INTO extracted_values (
    id, field_id, article_id, raw_quote, normalized_value_json, page,
    confidence, status, actor_type, actor_name, created_at, updated_at
)
SELECT legacy.id, legacy.field_id, legacy.article_id, evidence.quoted_text,
       legacy.value_json, evidence.page_start, legacy.confidence,
       CASE legacy.status
           WHEN 'verified' THEN 'confirmed'
           WHEN 'pending_review' THEN 'needs_review'
           ELSE legacy.status
       END,
       legacy.actor_type, legacy.actor_name, legacy.created_at, legacy.updated_at
FROM extracted_values_legacy AS legacy
LEFT JOIN evidence_spans AS evidence ON evidence.id = legacy.evidence_id;

CREATE TABLE extraction_value_evidence (
    extracted_value_id TEXT NOT NULL REFERENCES extracted_values(id) ON DELETE CASCADE,
    evidence_id TEXT NOT NULL REFERENCES evidence_spans(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL DEFAULT 'supports' CHECK (relation_type IN ('supports', 'contradicts')),
    PRIMARY KEY (extracted_value_id, evidence_id)
);
INSERT INTO extraction_value_evidence (extracted_value_id, evidence_id)
SELECT id, evidence_id FROM extracted_values_legacy WHERE evidence_id IS NOT NULL;
DROP TABLE extracted_values_legacy;
CREATE INDEX idx_extracted_values_article ON extracted_values(article_id, field_id);

CREATE TABLE ai_providers (
    id TEXT PRIMARY KEY,
    provider_type TEXT NOT NULL,
    display_name TEXT NOT NULL,
    base_url TEXT,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    config_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE ingestion_jobs (
    id TEXT PRIMARY KEY,
    article_id TEXT REFERENCES articles(id) ON DELETE CASCADE,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'paused', 'completed', 'failed', 'cancelled')),
    progress REAL NOT NULL DEFAULT 0 CHECK (progress >= 0 AND progress <= 1),
    input_json TEXT NOT NULL DEFAULT '{}',
    result_json TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE embedding_indexes (
    id TEXT PRIMARY KEY,
    article_id TEXT REFERENCES articles(id) ON DELETE CASCADE,
    document_page_id TEXT REFERENCES document_pages(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    dimensions INTEGER NOT NULL CHECK (dimensions > 0),
    storage_key TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE export_records (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES review_projects(id) ON DELETE SET NULL,
    format TEXT NOT NULL,
    output_path TEXT NOT NULL,
    output_sha256 TEXT,
    options_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE INDEX idx_protocol_versions_project ON protocol_versions(project_id, version_number);
CREATE INDEX idx_document_files_article ON document_files(article_id, imported_at);
CREATE INDEX idx_document_pages_file ON document_pages(document_file_id, page_number);
CREATE INDEX idx_document_blocks_page ON document_blocks(document_page_id, reading_order);
CREATE INDEX idx_extraction_runs_article ON extraction_runs(article_id, started_at);
CREATE INDEX idx_ingestion_jobs_status ON ingestion_jobs(status, created_at);
