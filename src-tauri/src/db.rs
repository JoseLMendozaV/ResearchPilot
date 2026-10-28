use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{AppSettings, BootstrapData, CreateProjectInput, DashboardStats, ReviewProject},
};

const MIGRATIONS: &[(i64, &str, &str)] = &[
    (
        1,
        "initial scientific review schema",
        include_str!("../migrations/0001_initial.sql"),
    ),
    (
        2,
        "master specification alignment",
        include_str!("../migrations/0002_master_spec_alignment.sql"),
    ),
    (
        3,
        "library import foundation",
        include_str!("../migrations/0003_library_import.sql"),
    ),
    (
        4,
        "bibliography import provenance",
        include_str!("../migrations/0004_bibliography_import.sql"),
    ),
    (
        5,
        "scientific extraction, master record and job queue",
        include_str!("../migrations/0005_scientific_extraction.sql"),
    ),
    (
        6,
        "local RAG index, assistant and AI runs",
        include_str!("../migrations/0006_local_rag.sql"),
    ),
    (
        7,
        "optional external AI gateway",
        include_str!("../migrations/0007_ai_gateway.sql"),
    ),
    (
        8,
        "systematic review methodology modules",
        include_str!("../migrations/0008_review_methodology.sql"),
    ),
    (
        9,
        "update check preference",
        include_str!("../migrations/0009_updates.sql"),
    ),
    (
        10,
        "synthesis drafts",
        include_str!("../migrations/0010_synthesis.sql"),
    ),
];
const REVIEW_TYPES: [&str; 3] = ["systematic_review", "systematic_mapping", "scoping_review"];

#[derive(Debug, Clone)]
pub struct Database {
    pub(crate) path: PathBuf,
    pub(crate) storage_dir: PathBuf,
}

impl Database {
    pub fn initialize(path: impl AsRef<Path>) -> AppResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let storage_dir = path
            .parent()
            .ok_or_else(|| AppError::Initialization("Ruta de base de datos inválida".into()))?
            .to_path_buf();
        let database = Self { path, storage_dir };
        let mut connection = database.open()?;
        database.migrate(&mut connection)?;
        Ok(database)
    }

    pub(crate) fn open(&self) -> AppResult<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        Ok(connection)
    }

    fn migrate(&self, connection: &mut Connection) -> AppResult<()> {
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                description TEXT NOT NULL,
                applied_at TEXT NOT NULL
            );",
        )?;

        for (version, description, sql) in MIGRATIONS {
            let applied: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                [version],
                |row| row.get(0),
            )?;

            if !applied {
                let transaction = connection.transaction()?;
                transaction.execute_batch(sql)?;
                transaction.execute(
                    "INSERT INTO schema_migrations (version, description, applied_at) VALUES (?1, ?2, ?3)",
                    params![version, description, now()],
                )?;
                transaction.commit()?;
            }
        }

        Ok(())
    }

    pub fn bootstrap(&self) -> AppResult<BootstrapData> {
        Ok(BootstrapData {
            settings: self.settings()?,
            projects: self.projects()?,
            stats: self.stats()?,
            database_path: self.path.to_string_lossy().into_owned(),
            articles: self.library_articles(None)?,
            collections: self.library_collections()?,
        })
    }

    pub fn settings(&self) -> AppResult<AppSettings> {
        let connection = self.open()?;
        connection
            .query_row(
                "SELECT researcher_name, default_language, appearance, ai_provider,
                        local_only, allow_external_processing, ollama_base_url, ollama_model,
                        auto_extract_on_import, ocr_enabled, ocr_languages, grobid_enabled,
                        grobid_url, extraction_timeout_seconds, extractor_python_path,
                        ollama_embedding_model, ollama_extraction_model, auto_index_after_extraction,
                        rag_mode, rag_top_k, rag_context_tokens, ai_timeout_seconds,
                        openai_model, anthropic_model, external_monthly_budget_usd, external_max_output_tokens,
                        check_updates_on_start
                 FROM app_settings WHERE id = 1",
                [],
                |row| {
                    Ok(AppSettings {
                        researcher_name: row.get(0)?,
                        default_language: row.get(1)?,
                        appearance: row.get(2)?,
                        ai_provider: row.get(3)?,
                        local_only: row.get(4)?,
                        allow_external_processing: row.get(5)?,
                        ollama_base_url: row.get(6)?,
                        ollama_model: row.get(7)?,
                        auto_extract_on_import: row.get(8)?,
                        ocr_enabled: row.get(9)?,
                        ocr_languages: row.get(10)?,
                        grobid_enabled: row.get(11)?,
                        grobid_url: row.get(12)?,
                        extraction_timeout_seconds: row.get(13)?,
                        extractor_python_path: row.get(14)?,
                        ollama_embedding_model: row.get(15)?,
                        ollama_extraction_model: row.get(16)?,
                        auto_index_after_extraction: row.get(17)?,
                        rag_mode: row.get(18)?,
                        rag_top_k: row.get(19)?,
                        rag_context_tokens: row.get(20)?,
                        ai_timeout_seconds: row.get(21)?,
                        openai_model: row.get(22)?,
                        anthropic_model: row.get(23)?,
                        external_monthly_budget_usd: row.get(24)?,
                        external_max_output_tokens: row.get(25)?,
                        check_updates_on_start: row.get(26)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn update_settings(&self, settings: AppSettings) -> AppResult<AppSettings> {
        validate_settings(&settings)?;
        let connection = self.open()?;
        connection.execute(
            "UPDATE app_settings
             SET researcher_name = ?1, default_language = ?2, appearance = ?3,
                 ai_provider = ?4, local_only = ?5, allow_external_processing = ?6,
                 ollama_base_url = ?7, ollama_model = ?8, updated_at = ?9,
                 auto_extract_on_import = ?10, ocr_enabled = ?11, ocr_languages = ?12,
                 grobid_enabled = ?13, grobid_url = ?14, extraction_timeout_seconds = ?15,
                 extractor_python_path = ?16, ollama_embedding_model = ?17,
                 ollama_extraction_model = ?18, auto_index_after_extraction = ?19, rag_mode = ?20,
                 rag_top_k = ?21, rag_context_tokens = ?22, ai_timeout_seconds = ?23,
                 openai_model = ?24, anthropic_model = ?25, external_monthly_budget_usd = ?26,
                 external_max_output_tokens = ?27, check_updates_on_start = ?28
             WHERE id = 1",
            params![
                settings.researcher_name.trim(),
                settings.default_language,
                settings.appearance,
                settings.ai_provider,
                settings.local_only,
                settings.allow_external_processing,
                settings.ollama_base_url.trim(),
                settings.ollama_model.trim(),
                now(),
                settings.auto_extract_on_import,
                settings.ocr_enabled,
                settings.ocr_languages.trim(),
                settings.grobid_enabled,
                settings.grobid_url.trim().trim_end_matches('/'),
                settings.extraction_timeout_seconds,
                settings.extractor_python_path.trim(),
                settings.ollama_embedding_model.trim(),
                settings.ollama_extraction_model.trim(),
                settings.auto_index_after_extraction,
                settings.rag_mode,
                settings.rag_top_k,
                settings.rag_context_tokens,
                settings.ai_timeout_seconds,
                settings.openai_model.trim(),
                settings.anthropic_model.trim(),
                settings.external_monthly_budget_usd,
                settings.external_max_output_tokens,
                settings.check_updates_on_start,
            ],
        )?;
        self.settings()
    }

    pub fn projects(&self) -> AppResult<Vec<ReviewProject>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT id, name, description, review_type, status, created_at, updated_at
             FROM review_projects ORDER BY updated_at DESC",
        )?;
        let projects = statement
            .query_map([], |row| {
                Ok(ReviewProject {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    review_type: row.get(3)?,
                    status: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(projects)
    }

    pub fn create_project(&self, input: CreateProjectInput) -> AppResult<ReviewProject> {
        let name = input.name.trim();
        if name.len() < 3 || name.len() > 140 {
            return Err(AppError::Validation(
                "El nombre debe tener entre 3 y 140 caracteres".into(),
            ));
        }
        if !REVIEW_TYPES.contains(&input.review_type.as_str()) {
            return Err(AppError::Validation(
                "Tipo de revisión no reconocido".into(),
            ));
        }

        let timestamp = now();
        let project = ReviewProject {
            id: Uuid::new_v4().to_string(),
            name: name.to_owned(),
            description: input.description.trim().to_owned(),
            review_type: input.review_type,
            status: "draft".into(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
        };
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO review_projects
             (id, name, description, review_type, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                project.id,
                project.name,
                project.description,
                project.review_type,
                project.status,
                project.created_at,
                project.updated_at,
            ],
        )?;
        transaction.execute(
            "INSERT INTO audit_events
             (id, project_id, entity_type, entity_id, action, actor_type, actor_name, after_json, created_at)
             VALUES (?1, ?2, 'review_project', ?2, 'created', 'human', 'local-user', ?3, ?4)",
            params![
                Uuid::new_v4().to_string(),
                project.id,
                serde_json::to_string(&project).map_err(|error| AppError::Initialization(error.to_string()))?,
                now(),
            ],
        )?;
        transaction.commit()?;
        Ok(project)
    }

    pub fn stats(&self) -> AppResult<DashboardStats> {
        let connection = self.open()?;
        Ok(DashboardStats {
            projects: count(&connection, "review_projects")?,
            articles: connection.query_row(
                "SELECT COUNT(*) FROM articles WHERE deleted_at IS NULL",
                [],
                |row| row.get(0),
            )?,
            research_questions: count(&connection, "research_questions")?,
            evidence_spans: count(&connection, "evidence_spans")?,
            active_jobs: connection.query_row(
                "SELECT COUNT(*) FROM ingestion_jobs WHERE status IN ('queued', 'running', 'paused')",
                [],
                |row| row.get(0),
            )?,
            values_to_review: connection.query_row(
                "SELECT COUNT(*) FROM master_record_values v JOIN articles a ON a.id = v.article_id
                 WHERE v.superseded_by_run_id IS NULL AND a.deleted_at IS NULL
                   AND v.status IN ('proposed', 'needs_review')",
                [],
                |row| row.get(0),
            )?,
        })
    }
}

fn count(connection: &Connection, table: &str) -> AppResult<i64> {
    // Only called with compile-time table names declared above.
    let query = format!("SELECT COUNT(*) FROM {table}");
    Ok(connection.query_row(&query, [], |row| row.get(0))?)
}

fn validate_settings(settings: &AppSettings) -> AppResult<()> {
    if !["es", "en"].contains(&settings.default_language.as_str()) {
        return Err(AppError::Validation("Idioma no soportado".into()));
    }
    if !["light", "dark", "system"].contains(&settings.appearance.as_str()) {
        return Err(AppError::Validation("Apariencia no reconocida".into()));
    }
    if !["ollama", "openai", "anthropic"].contains(&settings.ai_provider.as_str()) {
        return Err(AppError::Validation("Proveedor de IA no reconocido".into()));
    }
    if settings.ollama_base_url.trim().is_empty() {
        return Err(AppError::Validation(
            "La URL de Ollama no puede estar vacía".into(),
        ));
    }
    if settings.local_only && settings.allow_external_processing {
        return Err(AppError::Validation(
            "El procesamiento externo debe estar desactivado en modo solo local".into(),
        ));
    }
    if !crate::worker::is_loopback_url(&settings.ollama_base_url)
        && !(settings.allow_external_processing && !settings.local_only)
    {
        return Err(AppError::Validation(
            "Un Ollama fuera de este equipo requiere desactivar el modo solo local y autorizar el procesamiento externo".into(),
        ));
    }
    if !["hybrid", "lexical", "semantic"].contains(&settings.rag_mode.as_str()) {
        return Err(AppError::Validation(
            "Modo de búsqueda no reconocido".into(),
        ));
    }
    if !(1..=30).contains(&settings.rag_top_k)
        || !(1000..=64000).contains(&settings.rag_context_tokens)
    {
        return Err(AppError::Validation(
            "Usa entre 1 y 30 fragmentos y un presupuesto de 1000 a 64000 tokens".into(),
        ));
    }
    if !settings.external_monthly_budget_usd.is_finite()
        || settings.external_monthly_budget_usd < 0.0
    {
        return Err(AppError::Validation(
            "El presupuesto mensual no puede ser negativo".into(),
        ));
    }
    if !(256..=64000).contains(&settings.external_max_output_tokens) {
        return Err(AppError::Validation(
            "El límite de tokens de salida debe estar entre 256 y 64000".into(),
        ));
    }
    if !(10..=3600).contains(&settings.ai_timeout_seconds) {
        return Err(AppError::Validation(
            "El tiempo máximo de IA debe estar entre 10 y 3600 segundos".into(),
        ));
    }
    let languages = settings.ocr_languages.trim();
    if languages.is_empty()
        || languages.split('+').any(|code| {
            !(3..=16).contains(&code.len())
                || !code
                    .chars()
                    .all(|character| character.is_ascii_alphabetic() || character == '_')
        })
    {
        return Err(AppError::Validation(
            "Idiomas de OCR inválidos; usa códigos Tesseract como eng o eng+spa".into(),
        ));
    }
    if !(30..=7200).contains(&settings.extraction_timeout_seconds) {
        return Err(AppError::Validation(
            "El tiempo máximo de extracción debe estar entre 30 y 7200 segundos".into(),
        ));
    }
    let grobid_url = settings.grobid_url.trim();
    if !(grobid_url.starts_with("http://") || grobid_url.starts_with("https://")) {
        return Err(AppError::Validation(
            "La URL de GROBID debe usar http o https".into(),
        ));
    }
    if settings.grobid_enabled
        && !crate::worker::is_loopback_url(grobid_url)
        && !(settings.allow_external_processing && !settings.local_only)
    {
        return Err(AppError::Validation(
            "Un GROBID fuera de este equipo requiere desactivar el modo solo local y autorizar el procesamiento externo".into(),
        ));
    }
    let python = settings.extractor_python_path.trim();
    if !python.is_empty() && !Path::new(python).is_absolute() {
        return Err(AppError::Validation(
            "La ruta de Python debe ser absoluta o quedar vacía".into(),
        ));
    }
    Ok(())
}

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn test_database() -> (tempfile::TempDir, Database) {
        let directory = tempdir().expect("temp directory");
        let database = Database::initialize(directory.path().join("test.sqlite3"))
            .expect("database initializes");
        (directory, database)
    }

    #[test]
    fn initializes_local_first_defaults_and_all_workflow_tables() {
        let (_directory, database) = test_database();
        let snapshot = database.bootstrap().expect("bootstrap data");
        assert!(snapshot.settings.local_only);
        assert!(!snapshot.settings.allow_external_processing);
        assert_eq!(snapshot.settings.ai_provider, "ollama");
        assert!(snapshot.settings.ollama_model.is_empty());

        let connection = database.open().expect("connection");
        for table in [
            "control_articles",
            "research_questions",
            "search_strategies",
            "eligibility_criteria",
            "quality_criteria",
            "extraction_fields",
            "evidence_spans",
            "audit_events",
            "protocol_versions",
            "document_files",
            "document_pages",
            "extraction_value_evidence",
            "reviewers",
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                    [table],
                    |row| row.get(0),
                )
                .expect("table lookup");
            assert!(exists, "missing table {table}");
        }
    }

    #[test]
    fn supports_multiple_and_contradictory_evidence_per_extracted_value() {
        let (_directory, database) = test_database();
        let connection = database.open().expect("connection");
        let sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'extraction_value_evidence'",
                [],
                |row| row.get(0),
            )
            .expect("evidence relation schema");
        assert!(sql.contains("contradicts"));

        let extracted_values_sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'extracted_values'",
                [],
                |row| row.get(0),
            )
            .expect("extracted values schema");
        assert!(!extracted_values_sql.contains("UNIQUE (field_id, article_id)"));
    }

    #[test]
    fn creates_a_project_and_audit_event_atomically() {
        let (_directory, database) = test_database();
        let project = database
            .create_project(CreateProjectInput {
                name: "Mapeo de evidencia".into(),
                description: "Proyecto de prueba".into(),
                review_type: "systematic_mapping".into(),
            })
            .expect("project created");

        assert_eq!(database.stats().expect("stats").projects, 1);
        let connection = database.open().expect("connection");
        let audit_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE project_id = ?1 AND action = 'created'",
                [project.id],
                |row| row.get(0),
            )
            .expect("audit count");
        assert_eq!(audit_count, 1);
    }

    #[test]
    fn refuses_external_processing_while_local_only_is_enabled() {
        let (_directory, database) = test_database();
        let mut settings = database.settings().expect("settings");
        settings.allow_external_processing = true;
        let result = database.update_settings(settings);
        assert!(matches!(result, Err(AppError::Validation(_))));
    }
}
