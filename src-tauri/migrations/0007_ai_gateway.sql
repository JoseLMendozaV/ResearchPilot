-- Fase 5: proveedores externos opcionales (OpenAI, Anthropic) con consentimiento,
-- presupuesto, costes registrados y artículos confidenciales.

ALTER TABLE app_settings ADD COLUMN openai_model TEXT NOT NULL DEFAULT '';
ALTER TABLE app_settings ADD COLUMN anthropic_model TEXT NOT NULL DEFAULT '';
ALTER TABLE app_settings ADD COLUMN external_monthly_budget_usd REAL NOT NULL DEFAULT 0 CHECK (external_monthly_budget_usd >= 0);
ALTER TABLE app_settings ADD COLUMN external_max_output_tokens INTEGER NOT NULL DEFAULT 4000 CHECK (external_max_output_tokens BETWEEN 256 AND 64000);

-- Un artículo confidencial nunca se envía a un proveedor externo.
ALTER TABLE articles ADD COLUMN confidential INTEGER NOT NULL DEFAULT 0 CHECK (confidential IN (0, 1));

ALTER TABLE ai_runs ADD COLUMN cost_usd REAL;
ALTER TABLE ai_runs ADD COLUMN estimated_cost_usd REAL;

-- Precios por millón de tokens. Las filas 'reference' son valores públicos de
-- referencia con fecha; el usuario puede corregirlos ('user').
CREATE TABLE ai_model_pricing (
    provider TEXT NOT NULL CHECK (provider IN ('openai', 'anthropic')),
    model TEXT NOT NULL,
    input_usd_per_mtok REAL NOT NULL CHECK (input_usd_per_mtok >= 0),
    output_usd_per_mtok REAL NOT NULL CHECK (output_usd_per_mtok >= 0),
    source TEXT NOT NULL DEFAULT 'user' CHECK (source IN ('reference', 'user')),
    updated_at TEXT NOT NULL,
    PRIMARY KEY (provider, model)
);

INSERT INTO ai_model_pricing (provider, model, input_usd_per_mtok, output_usd_per_mtok, source, updated_at) VALUES
('anthropic', 'claude-fable-5-1', 10, 50, 'reference', '2026-10-06'),
('anthropic', 'claude-fable-5', 10, 50, 'reference', '2026-10-06'),
('anthropic', 'claude-opus-5-5', 4, 20, 'reference', '2026-10-06'),
('anthropic', 'claude-opus-5', 5, 25, 'reference', '2026-10-06'),
('anthropic', 'claude-opus-4-8', 5, 25, 'reference', '2026-10-06'),
('anthropic', 'claude-sonnet-5-5', 2, 10, 'reference', '2026-10-06'),
('anthropic', 'claude-sonnet-5', 2, 10, 'reference', '2026-10-06'),
('anthropic', 'claude-sonnet-4-6', 3, 15, 'reference', '2026-10-06'),
('anthropic', 'claude-haiku-5-5', 0.10, 0.50, 'reference', '2026-10-06'),
('anthropic', 'claude-haiku-4-5', 1, 5, 'reference', '2026-10-06');

CREATE INDEX idx_ai_runs_external ON ai_runs(external_processing_authorized, created_at);
