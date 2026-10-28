ALTER TABLE document_files ADD COLUMN source_path TEXT;

CREATE INDEX idx_articles_active_updated
ON articles(deleted_at, updated_at);

CREATE INDEX idx_document_files_primary
ON document_files(article_id, is_primary, deleted_at);
