CREATE TABLE import_batches (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL,
    source_sha256 TEXT NOT NULL,
    format TEXT NOT NULL CHECK (format IN ('ris', 'bibtex', 'csv')),
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'completed_with_errors', 'failed')),
    imported_count INTEGER NOT NULL DEFAULT 0,
    duplicate_count INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    completed_at TEXT
);

CREATE TABLE source_records (
    id TEXT PRIMARY KEY,
    import_batch_id TEXT NOT NULL REFERENCES import_batches(id) ON DELETE CASCADE,
    article_id TEXT REFERENCES articles(id) ON DELETE SET NULL,
    source_key TEXT,
    raw_json TEXT NOT NULL,
    record_status TEXT NOT NULL CHECK (record_status IN ('imported', 'duplicate', 'error')),
    created_at TEXT NOT NULL
);

CREATE TABLE duplicate_candidates (
    id TEXT PRIMARY KEY,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    candidate_article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    reasons_json TEXT NOT NULL,
    similarity_score REAL CHECK (similarity_score IS NULL OR (similarity_score >= 0 AND similarity_score <= 1)),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'confirmed', 'dismissed')),
    created_at TEXT NOT NULL,
    resolved_at TEXT,
    UNIQUE (article_id, candidate_article_id)
);

CREATE INDEX idx_import_batches_created ON import_batches(created_at);
CREATE INDEX idx_source_records_batch ON source_records(import_batch_id, record_status);
CREATE INDEX idx_duplicate_candidates_status ON duplicate_candidates(status, created_at);
