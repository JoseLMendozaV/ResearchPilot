CREATE TABLE app_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    researcher_name TEXT NOT NULL DEFAULT '',
    default_language TEXT NOT NULL DEFAULT 'es',
    appearance TEXT NOT NULL DEFAULT 'system' CHECK (appearance IN ('light', 'dark', 'system')),
    ai_provider TEXT NOT NULL DEFAULT 'ollama' CHECK (ai_provider IN ('ollama', 'openai', 'anthropic')),
    local_only INTEGER NOT NULL DEFAULT 1 CHECK (local_only IN (0, 1)),
    allow_external_processing INTEGER NOT NULL DEFAULT 0 CHECK (allow_external_processing IN (0, 1)),
    ollama_base_url TEXT NOT NULL DEFAULT 'http://127.0.0.1:11434',
    ollama_model TEXT NOT NULL DEFAULT 'qwen3:8b',
    updated_at TEXT NOT NULL
);

INSERT INTO app_settings (id, updated_at) VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

CREATE TABLE review_projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    review_type TEXT NOT NULL CHECK (review_type IN ('systematic_review', 'systematic_mapping', 'scoping_review')),
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'active', 'completed', 'archived')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE articles (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    abstract_text TEXT,
    publication_year INTEGER,
    doi TEXT,
    authors_json TEXT NOT NULL DEFAULT '[]',
    keywords_json TEXT NOT NULL DEFAULT '[]',
    venue TEXT,
    reference_text TEXT,
    pdf_path TEXT,
    document_sha256 TEXT,
    metadata_source TEXT NOT NULL DEFAULT 'manual',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (doi),
    UNIQUE (document_sha256)
);

CREATE VIRTUAL TABLE articles_fts USING fts5(
    title,
    abstract_text,
    authors,
    keywords,
    content='articles',
    content_rowid='rowid'
);

CREATE TRIGGER articles_fts_insert AFTER INSERT ON articles BEGIN
    INSERT INTO articles_fts(rowid, title, abstract_text, authors, keywords)
    VALUES (new.rowid, new.title, coalesce(new.abstract_text, ''), new.authors_json, new.keywords_json);
END;

CREATE TRIGGER articles_fts_delete AFTER DELETE ON articles BEGIN
    INSERT INTO articles_fts(articles_fts, rowid, title, abstract_text, authors, keywords)
    VALUES ('delete', old.rowid, old.title, coalesce(old.abstract_text, ''), old.authors_json, old.keywords_json);
END;

CREATE TRIGGER articles_fts_update AFTER UPDATE ON articles BEGIN
    INSERT INTO articles_fts(articles_fts, rowid, title, abstract_text, authors, keywords)
    VALUES ('delete', old.rowid, old.title, coalesce(old.abstract_text, ''), old.authors_json, old.keywords_json);
    INSERT INTO articles_fts(rowid, title, abstract_text, authors, keywords)
    VALUES (new.rowid, new.title, coalesce(new.abstract_text, ''), new.authors_json, new.keywords_json);
END;

CREATE TABLE review_articles (
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    workflow_stage TEXT NOT NULL DEFAULT 'identified' CHECK (workflow_stage IN ('identified', 'screening', 'eligible', 'included', 'excluded')),
    source_database TEXT,
    imported_at TEXT NOT NULL,
    PRIMARY KEY (project_id, article_id)
);

-- Hoja Control: artículos clave usados para validar la estrategia de búsqueda.
CREATE TABLE control_articles (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    rationale TEXT NOT NULL DEFAULT '',
    display_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    UNIQUE (project_id, article_id)
);

-- Hoja RQs: preguntas de investigación y aspectos configurables.
CREATE TABLE research_questions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    question TEXT NOT NULL,
    objective TEXT NOT NULL DEFAULT '',
    display_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (project_id, code)
);

CREATE TABLE rq_aspects (
    id TEXT PRIMARY KEY,
    research_question_id TEXT NOT NULL REFERENCES research_questions(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    display_order INTEGER NOT NULL DEFAULT 0
);

-- Hoja Cadena: cadenas por fuente y cada ejecución reproducible.
CREATE TABLE search_strategies (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    database_name TEXT NOT NULL,
    query_text TEXT NOT NULL,
    query_date TEXT NOT NULL,
    result_count INTEGER NOT NULL DEFAULT 0 CHECK (result_count >= 0),
    notes TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE TABLE search_imports (
    id TEXT PRIMARY KEY,
    search_strategy_id TEXT NOT NULL REFERENCES search_strategies(id) ON DELETE CASCADE,
    source_path TEXT NOT NULL,
    source_sha256 TEXT NOT NULL,
    imported_count INTEGER NOT NULL DEFAULT 0,
    duplicate_count INTEGER NOT NULL DEFAULT 0,
    imported_at TEXT NOT NULL
);

-- Hoja I_E: criterios configurables y decisión humana/IA separada.
CREATE TABLE eligibility_criteria (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    criterion_type TEXT NOT NULL CHECK (criterion_type IN ('inclusion', 'exclusion')),
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    display_order INTEGER NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    UNIQUE (project_id, code)
);

CREATE TABLE screening_decisions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    phase TEXT NOT NULL CHECK (phase IN ('metadata', 'full_text')),
    decision TEXT NOT NULL CHECK (decision IN ('include', 'exclude', 'uncertain')),
    criterion_id TEXT REFERENCES eligibility_criteria(id) ON DELETE SET NULL,
    actor_type TEXT NOT NULL CHECK (actor_type IN ('human', 'ai')),
    actor_name TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    decided_at TEXT NOT NULL
);

-- Hoja Calidad: rúbrica y puntajes conservando quién evaluó.
CREATE TABLE quality_criteria (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    question TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1 CHECK (weight >= 0),
    max_score REAL NOT NULL DEFAULT 1 CHECK (max_score >= 0),
    display_order INTEGER NOT NULL DEFAULT 0,
    UNIQUE (project_id, code)
);

CREATE TABLE quality_scores (
    id TEXT PRIMARY KEY,
    criterion_id TEXT NOT NULL REFERENCES quality_criteria(id) ON DELETE CASCADE,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    score REAL NOT NULL,
    justification TEXT NOT NULL DEFAULT '',
    actor_type TEXT NOT NULL CHECK (actor_type IN ('human', 'ai')),
    actor_name TEXT NOT NULL,
    assessed_at TEXT NOT NULL,
    UNIQUE (criterion_id, article_id, actor_type, actor_name)
);

-- Hoja Extraccion: campos libres, vinculables a una RQ, y valores con evidencia.
CREATE TABLE extraction_fields (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    research_question_id TEXT REFERENCES research_questions(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    data_type TEXT NOT NULL DEFAULT 'text' CHECK (data_type IN ('text', 'number', 'boolean', 'date', 'category', 'list')),
    required INTEGER NOT NULL DEFAULT 0 CHECK (required IN (0, 1)),
    display_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE evidence_spans (
    id TEXT PRIMARY KEY,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    page_start INTEGER,
    page_end INTEGER,
    quoted_text TEXT NOT NULL,
    source_path TEXT,
    source_sha256 TEXT,
    locator_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    CHECK (page_start IS NULL OR page_start > 0),
    CHECK (page_end IS NULL OR page_end >= page_start)
);

CREATE TABLE extracted_values (
    id TEXT PRIMARY KEY,
    field_id TEXT NOT NULL REFERENCES extraction_fields(id) ON DELETE CASCADE,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    value_json TEXT NOT NULL,
    evidence_id TEXT REFERENCES evidence_spans(id) ON DELETE SET NULL,
    confidence REAL CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
    status TEXT NOT NULL DEFAULT 'pending_review' CHECK (status IN ('pending_review', 'verified', 'rejected')),
    actor_type TEXT NOT NULL CHECK (actor_type IN ('human', 'ai', 'import')),
    actor_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (field_id, article_id)
);

CREATE TABLE ai_runs (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES review_projects(id) ON DELETE CASCADE,
    article_id TEXT REFERENCES articles(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    operation TEXT NOT NULL,
    prompt_sha256 TEXT NOT NULL,
    input_sha256 TEXT,
    output_json TEXT NOT NULL,
    external_processing_authorized INTEGER NOT NULL DEFAULT 0 CHECK (external_processing_authorized IN (0, 1)),
    created_at TEXT NOT NULL
);

CREATE TABLE audit_events (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES review_projects(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL,
    entity_id TEXT,
    action TEXT NOT NULL,
    actor_type TEXT NOT NULL CHECK (actor_type IN ('human', 'ai', 'system', 'import')),
    actor_name TEXT NOT NULL,
    before_json TEXT,
    after_json TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_review_articles_stage ON review_articles(project_id, workflow_stage);
CREATE INDEX idx_control_articles_project ON control_articles(project_id, display_order);
CREATE INDEX idx_rqs_project ON research_questions(project_id, display_order);
CREATE INDEX idx_search_strategies_project ON search_strategies(project_id, query_date);
CREATE INDEX idx_screening_article ON screening_decisions(project_id, article_id, phase);
CREATE INDEX idx_quality_scores_article ON quality_scores(article_id);
CREATE INDEX idx_extracted_values_article ON extracted_values(article_id);
CREATE INDEX idx_evidence_article ON evidence_spans(article_id, page_start);
CREATE INDEX idx_audit_project ON audit_events(project_id, created_at);
