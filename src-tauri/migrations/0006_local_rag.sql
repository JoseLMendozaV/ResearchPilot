-- Fase 4: Ollama, índice híbrido (FTS5 + vectorial) y asistente con citas verificables.

ALTER TABLE app_settings ADD COLUMN ollama_embedding_model TEXT NOT NULL DEFAULT '';
ALTER TABLE app_settings ADD COLUMN ollama_extraction_model TEXT NOT NULL DEFAULT '';
ALTER TABLE app_settings ADD COLUMN auto_index_after_extraction INTEGER NOT NULL DEFAULT 1 CHECK (auto_index_after_extraction IN (0, 1));
ALTER TABLE app_settings ADD COLUMN rag_mode TEXT NOT NULL DEFAULT 'hybrid' CHECK (rag_mode IN ('hybrid', 'lexical', 'semantic'));
ALTER TABLE app_settings ADD COLUMN rag_top_k INTEGER NOT NULL DEFAULT 8 CHECK (rag_top_k BETWEEN 1 AND 30);
ALTER TABLE app_settings ADD COLUMN rag_context_tokens INTEGER NOT NULL DEFAULT 6000 CHECK (rag_context_tokens BETWEEN 1000 AND 64000);
ALTER TABLE app_settings ADD COLUMN ai_timeout_seconds INTEGER NOT NULL DEFAULT 300 CHECK (ai_timeout_seconds BETWEEN 10 AND 3600);

-- Reemplazada por chunk_embeddings; nunca tuvo datos.
DROP TABLE embedding_indexes;

-- Fragmentos recuperables: texto literal por página y sección, con las cajas
-- de los bloques de origen para resaltar la cita en el PDF.
CREATE TABLE document_chunks (
    id TEXT PRIMARY KEY,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    document_file_id TEXT NOT NULL REFERENCES document_files(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    page_number INTEGER NOT NULL CHECK (page_number > 0),
    section TEXT,
    canonical_section TEXT,
    text TEXT NOT NULL,
    spans_json TEXT NOT NULL DEFAULT '[]',
    content_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (document_file_id, chunk_index)
);

CREATE VIRTUAL TABLE document_chunks_fts USING fts5(
    text,
    section,
    content='document_chunks',
    content_rowid='rowid',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER document_chunks_fts_insert AFTER INSERT ON document_chunks BEGIN
    INSERT INTO document_chunks_fts(rowid, text, section) VALUES (new.rowid, new.text, coalesce(new.section, ''));
END;
CREATE TRIGGER document_chunks_fts_delete AFTER DELETE ON document_chunks BEGIN
    INSERT INTO document_chunks_fts(document_chunks_fts, rowid, text, section) VALUES ('delete', old.rowid, old.text, coalesce(old.section, ''));
END;
CREATE TRIGGER document_chunks_fts_update AFTER UPDATE ON document_chunks BEGIN
    INSERT INTO document_chunks_fts(document_chunks_fts, rowid, text, section) VALUES ('delete', old.rowid, old.text, coalesce(old.section, ''));
    INSERT INTO document_chunks_fts(rowid, text, section) VALUES (new.rowid, new.text, coalesce(new.section, ''));
END;

-- Vectores por contenido y modelo: re-fragmentar no obliga a recalcular
-- embeddings de un texto idéntico. Vectores f32 little-endian normalizados.
CREATE TABLE chunk_embeddings (
    content_sha256 TEXT NOT NULL,
    model TEXT NOT NULL,
    dimensions INTEGER NOT NULL CHECK (dimensions > 0),
    vector BLOB NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (content_sha256, model)
);

CREATE TABLE assistant_conversations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    scope_type TEXT NOT NULL CHECK (scope_type IN ('library', 'collection', 'article')),
    scope_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE TABLE assistant_messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES assistant_conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'answered' CHECK (status IN ('question', 'answered', 'not_found', 'unsupported', 'failed')),
    citations_json TEXT NOT NULL DEFAULT '[]',
    sources_json TEXT NOT NULL DEFAULT '[]',
    ai_run_id TEXT REFERENCES ai_runs(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL
);

ALTER TABLE ai_runs ADD COLUMN status TEXT NOT NULL DEFAULT 'completed' CHECK (status IN ('completed', 'failed', 'cancelled'));
ALTER TABLE ai_runs ADD COLUMN prompt_version TEXT;
ALTER TABLE ai_runs ADD COLUMN duration_ms INTEGER;
ALTER TABLE ai_runs ADD COLUMN prompt_tokens INTEGER;
ALTER TABLE ai_runs ADD COLUMN completion_tokens INTEGER;
ALTER TABLE ai_runs ADD COLUMN attempts INTEGER NOT NULL DEFAULT 1;
ALTER TABLE ai_runs ADD COLUMN error_message TEXT;

CREATE INDEX idx_chunks_article ON document_chunks(article_id, page_number, chunk_index);
CREATE INDEX idx_chunks_sha ON document_chunks(content_sha256);
CREATE INDEX idx_chunk_embeddings_model ON chunk_embeddings(model);
CREATE INDEX idx_assistant_messages_conversation ON assistant_messages(conversation_id, created_at);
CREATE INDEX idx_assistant_conversations_updated ON assistant_conversations(deleted_at, updated_at);
CREATE INDEX idx_ai_runs_created ON ai_runs(created_at);
