-- Fase 8: borradores editables de síntesis (metodología, resultados, limitaciones).
CREATE TABLE synthesis_drafts (
    project_id TEXT NOT NULL REFERENCES review_projects(id) ON DELETE CASCADE,
    section TEXT NOT NULL CHECK (section IN ('methods', 'results', 'limitations')),
    content TEXT NOT NULL,
    generated_content TEXT NOT NULL,
    generated_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (project_id, section)
);
