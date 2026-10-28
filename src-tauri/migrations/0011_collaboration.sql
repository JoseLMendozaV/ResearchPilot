-- Phase 9: deferred collaboration, Zotero import and performance indexes.

-- Links from external libraries (Zotero) to local articles, so re-imports are idempotent.
CREATE TABLE external_links (
    source TEXT NOT NULL CHECK (source IN ('zotero')),
    external_key TEXT NOT NULL,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (source, external_key)
);
CREATE INDEX idx_external_links_article ON external_links(article_id);

-- Reviewer exchange packages applied to a project; the hash makes re-imports no-ops.
CREATE TABLE exchange_imports (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    package_sha256 TEXT NOT NULL,
    origin_reviewer_id TEXT,
    origin_reviewer_name TEXT NOT NULL,
    summary_json TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    UNIQUE (project_id, package_sha256)
);

-- Lookups used by large corpora (matching, conflicts and evidence joins).
CREATE INDEX IF NOT EXISTS idx_articles_doi_lower ON articles(lower(doi));
CREATE INDEX IF NOT EXISTS idx_articles_title_year ON articles(lower(trim(title)), publication_year);
CREATE INDEX IF NOT EXISTS idx_reviewer_conflicts_lookup ON reviewer_conflicts(project_id, article_id, conflict_type, status);
CREATE INDEX IF NOT EXISTS idx_quality_scores_criterion ON quality_scores(criterion_id, article_id);
CREATE INDEX IF NOT EXISTS idx_extraction_value_evidence_evidence ON extraction_value_evidence(evidence_id);
CREATE INDEX IF NOT EXISTS idx_screening_reviewer ON screening_decisions(project_id, reviewer_id);
