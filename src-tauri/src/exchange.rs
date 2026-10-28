//! Reviewer exchange packages for deferred (offline) double review.
//!
//! A package is a JSON file with a project's setup (protocol, criteria, RQs and
//! fields, quality questions), its corpus metadata and every human decision, score
//! and extraction value recorded on the exporting machine. Ids are preserved, so
//! importing a colleague's package adds only what is new; the usual reconciliation
//! then finalizes agreements and opens conflicts for disagreements.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::Path,
};

use rusqlite::{
    Connection, OptionalExtension, ToSql, params, params_from_iter,
    types::{Value as SqlValue, ValueRef},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    db::{Database, SCHEMA_VERSION, now},
    error::{AppError, AppResult},
    quality::{self, CONSENSUS},
    review::{Actor, active_reviewer, audit, ensure_project, local_reviewer},
    screening::{self, refresh_stage},
};

pub const EXCHANGE_FORMAT: &str = "researchpilot-exchange/1";
const MAX_PACKAGE_BYTES: u64 = 200 * 1024 * 1024;
const RESOLUTION_PREFIX: &str = "Resolución de conflicto:";

type Record = Map<String, Value>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangePackage {
    pub format: String,
    pub app_version: String,
    pub schema_version: i64,
    pub created_at: String,
    pub project_id: String,
    pub project_name: String,
    pub reviewer_id: String,
    pub reviewer_name: String,
    pub tables: BTreeMap<String, Vec<Record>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeExportResult {
    pub path: String,
    pub sha256: String,
    pub articles: usize,
    pub decisions: usize,
    pub scores: usize,
    pub values: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeImportSummary {
    pub project_id: String,
    pub project_name: String,
    pub reviewer_name: String,
    pub already_imported: bool,
    pub project_created: bool,
    pub articles_matched: usize,
    pub articles_created: usize,
    pub decisions_added: usize,
    pub scores_added: usize,
    pub values_added: usize,
    pub open_conflicts: i64,
}

/// Tables in dependency order: a row's references are always inserted first.
const SETUP_TABLES: [&str; 9] = [
    "research_questions",
    "eligibility_criteria",
    "quality_criteria",
    "extraction_fields",
    "search_strategies",
    "protocol_versions",
    "protocol_amendments",
    "control_articles",
    "review_articles",
];

impl Database {
    pub fn export_exchange_package(
        &self,
        project_id: &str,
        destination: &Path,
    ) -> AppResult<ExchangeExportResult> {
        let connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let researcher: String = connection.query_row(
            "SELECT researcher_name FROM app_settings WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        if researcher.trim().is_empty() {
            return Err(AppError::Validation(
                "Escribe tu nombre en Configuración → Perfil local antes de intercambiar: identifica tus decisiones ante el otro revisor".into(),
            ));
        }
        let actor = local_reviewer(&connection, project_id)?;
        let project_name: String = connection.query_row(
            "SELECT name FROM review_projects WHERE id = ?1",
            [project_id],
            |row| row.get(0),
        )?;

        let mut tables = BTreeMap::new();
        let project = [&project_id as &dyn ToSql];
        let mut put = |table: &str, sql: &str| -> AppResult<()> {
            tables.insert(
                table.to_owned(),
                select_records(&connection, sql, &project)?,
            );
            Ok(())
        };
        put(
            "review_projects",
            "SELECT * FROM review_projects WHERE id = ?1",
        )?;
        put(
            "reviewers",
            "SELECT id, display_name, created_at FROM reviewers WHERE id IN (
                SELECT reviewer_id FROM project_reviewers WHERE project_id = ?1
                UNION SELECT reviewer_id FROM screening_decisions WHERE project_id = ?1
                UNION SELECT s.reviewer_id FROM quality_scores s JOIN quality_criteria c ON c.id = s.criterion_id WHERE c.project_id = ?1
                UNION SELECT v.reviewer_id FROM extracted_values v JOIN extraction_fields f ON f.id = v.field_id WHERE f.project_id = ?1)",
        )?;
        put(
            "project_reviewers",
            "SELECT * FROM project_reviewers WHERE project_id = ?1",
        )?;
        put(
            "articles",
            "SELECT id, title, abstract_text, publication_year, doi, authors_json, keywords_json, venue,
                    reference_text, document_sha256, metadata_source, created_at, updated_at
             FROM articles WHERE id IN (SELECT article_id FROM review_articles WHERE project_id = ?1)",
        )?;
        for table in SETUP_TABLES {
            let sql = match table {
                "research_questions" => {
                    "SELECT * FROM research_questions WHERE project_id = ?1 ORDER BY parent_id IS NOT NULL, display_order".to_owned()
                }
                "protocol_amendments" => {
                    "SELECT a.* FROM protocol_amendments a JOIN protocol_versions v ON v.id = a.protocol_version_id WHERE v.project_id = ?1".to_owned()
                }
                _ => format!("SELECT * FROM {table} WHERE project_id = ?1"),
            };
            put(table, &sql)?;
        }
        put(
            "evidence_spans",
            "SELECT e.* FROM evidence_spans e WHERE e.id IN (
                SELECT link.evidence_id FROM extraction_value_evidence link
                JOIN extracted_values v ON v.id = link.extracted_value_id
                JOIN extraction_fields f ON f.id = v.field_id WHERE f.project_id = ?1
                UNION SELECT evidence_id FROM screening_decisions WHERE project_id = ?1)",
        )?;
        put(
            "screening_decisions",
            "SELECT * FROM screening_decisions WHERE project_id = ?1 AND reviewer_id IS NOT NULL ORDER BY decided_at",
        )?;
        put(
            "quality_scores",
            "SELECT s.* FROM quality_scores s JOIN quality_criteria c ON c.id = s.criterion_id
             WHERE c.project_id = ?1 AND s.actor_type = 'human'",
        )?;
        put(
            "extracted_values",
            "SELECT v.* FROM extracted_values v JOIN extraction_fields f ON f.id = v.field_id WHERE f.project_id = ?1",
        )?;
        put(
            "extraction_value_evidence",
            "SELECT link.* FROM extraction_value_evidence link
             JOIN extracted_values v ON v.id = link.extracted_value_id
             JOIN extraction_fields f ON f.id = v.field_id WHERE f.project_id = ?1",
        )?;

        let count = |table: &str| tables.get(table).map_or(0, Vec::len);
        let result = ExchangeExportResult {
            path: destination.to_string_lossy().into_owned(),
            sha256: String::new(),
            articles: count("articles"),
            decisions: count("screening_decisions"),
            scores: count("quality_scores"),
            values: count("extracted_values"),
        };
        let package = ExchangePackage {
            format: EXCHANGE_FORMAT.into(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: SCHEMA_VERSION,
            created_at: now(),
            project_id: project_id.into(),
            project_name,
            reviewer_id: actor.reviewer_id.clone(),
            reviewer_name: actor.name.clone(),
            tables,
        };
        let bytes = serde_json::to_vec(&package).map_err(json_error)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, &bytes)?;
        let sha256 = hex(&bytes);
        audit(
            &connection,
            project_id,
            "exchange_package",
            &sha256,
            "exported",
            &actor,
            None,
            json!({ "path": result.path, "decisions": result.decisions, "scores": result.scores, "values": result.values }),
        )?;
        Ok(ExchangeExportResult { sha256, ..result })
    }

    pub fn import_exchange_package(&self, source: &Path) -> AppResult<ExchangeImportSummary> {
        let size = fs::metadata(source)?.len();
        if size == 0 || size > MAX_PACKAGE_BYTES {
            return Err(AppError::Validation(
                "El paquete de intercambio está vacío o es demasiado grande".into(),
            ));
        }
        let bytes = fs::read(source)?;
        let sha256 = hex(&bytes);
        let package: ExchangePackage = serde_json::from_slice(&bytes).map_err(|_| {
            AppError::Validation(
                "El archivo no es un paquete de intercambio de ResearchPilot".into(),
            )
        })?;
        if package.format != EXCHANGE_FORMAT {
            return Err(AppError::Validation(format!(
                "Formato de paquete no compatible: {}",
                package.format
            )));
        }
        if package.schema_version > SCHEMA_VERSION {
            return Err(AppError::Validation(
                "El paquete proviene de una versión más nueva de ResearchPilot; actualiza la app antes de importarlo".into(),
            ));
        }
        let project_rows = package.tables.get("review_projects").map_or(0, Vec::len);
        if project_rows != 1
            || text(&package.tables["review_projects"][0], "id")
                != Some(package.project_id.as_str())
        {
            return Err(AppError::Validation(
                "El paquete no describe una única revisión".into(),
            ));
        }

        let mut connection = self.open()?;
        let mut summary = ExchangeImportSummary {
            project_id: package.project_id.clone(),
            project_name: package.project_name.clone(),
            reviewer_name: package.reviewer_name.clone(),
            ..Default::default()
        };
        let already: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM exchange_imports WHERE project_id = ?1 AND package_sha256 = ?2)",
            params![package.project_id, sha256],
            |row| row.get(0),
        )?;
        if already {
            summary.already_imported = true;
            summary.open_conflicts = open_conflicts(&connection, &package.project_id)?;
            return Ok(summary);
        }

        let transaction = connection.transaction()?;
        let project_id = package.project_id.as_str();
        let mut columns = ColumnCache::default();
        let table = |name: &str| package.tables.get(name).map(Vec::as_slice).unwrap_or(&[]);

        // Project and reviewers. The colleague's profile is never local here.
        let mut project = table("review_projects")[0].clone();
        project.insert("active_reviewer_id".into(), Value::Null);
        project.insert("deleted_at".into(), Value::Null);
        summary.project_created = insert(&transaction, &mut columns, "review_projects", &project)?;
        for reviewer in table("reviewers") {
            let mut reviewer = reviewer.clone();
            reviewer.insert("is_local_profile".into(), json!(0));
            insert(&transaction, &mut columns, "reviewers", &reviewer)?;
        }
        let local = local_reviewer(&transaction, project_id)?;
        if local.reviewer_id != package.reviewer_id && local.name == package.reviewer_name {
            return Err(AppError::Validation(format!(
                "El paquete es de otro revisor con tu mismo nombre («{}»); usad nombres distintos en Configuración → Perfil local",
                local.name
            )));
        }
        for row in table("project_reviewers") {
            let mut row = row.clone();
            if text(&row, "reviewer_id") != Some(local.reviewer_id.as_str()) {
                row.insert("role".into(), json!("reviewer"));
            }
            insert(&transaction, &mut columns, "project_reviewers", &row)?;
        }

        // Articles: same id, then DOI, file hash, title and year; otherwise created
        // as metadata-only records (the PDF attaches itself when imported later).
        let mut article_map: HashMap<String, String> = HashMap::new();
        for article in table("articles") {
            let package_id = text(article, "id").unwrap_or_default().to_owned();
            match match_article(&transaction, article)? {
                Some(local_id) => {
                    summary.articles_matched += 1;
                    article_map.insert(package_id, local_id);
                }
                None => {
                    let mut article = article.clone();
                    article.insert("metadata_source".into(), json!("exchange"));
                    article.insert("updated_at".into(), json!(now()));
                    if insert(&transaction, &mut columns, "articles", &article)? {
                        summary.articles_created += 1;
                        article_map.insert(package_id.clone(), package_id);
                    }
                }
            }
        }
        let remap = |row: &Record| -> Option<Record> {
            let mut row = row.clone();
            if let Some(article_id) = text(&row, "article_id") {
                let local_id = article_map.get(article_id)?.clone();
                row.insert("article_id".into(), json!(local_id));
            }
            Some(row)
        };

        for name in SETUP_TABLES {
            for row in table(name) {
                if let Some(row) = remap(row) {
                    insert(&transaction, &mut columns, name, &row)?;
                }
            }
        }
        for row in table("evidence_spans") {
            if let Some(mut row) = remap(row) {
                let file = text(&row, "document_file_id").map(str::to_owned);
                if let Some(file) = file {
                    let exists: bool = transaction.query_row(
                        "SELECT EXISTS(SELECT 1 FROM document_files WHERE id = ?1)",
                        [file],
                        |r| r.get(0),
                    )?;
                    if !exists {
                        row.insert("document_file_id".into(), Value::Null);
                    }
                }
                insert(&transaction, &mut columns, "evidence_spans", &row)?;
            }
        }

        // Screening: raw reviewer decisions; finality is recomputed below.
        let mut touched_screening = BTreeSet::new();
        for row in table("screening_decisions") {
            if let Some(mut row) = remap(row) {
                row.insert("is_final".into(), json!(0));
                if !evidence_exists(&transaction, &row)? {
                    row.insert("evidence_id".into(), Value::Null);
                }
                if insert(&transaction, &mut columns, "screening_decisions", &row)? {
                    summary.decisions_added += 1;
                    touched_screening.insert((
                        text(&row, "article_id").unwrap_or_default().to_owned(),
                        text(&row, "phase").unwrap_or_default().to_owned(),
                    ));
                }
            }
        }
        let mode: String = transaction.query_row(
            "SELECT screening_mode FROM review_projects WHERE id = ?1",
            [project_id],
            |row| row.get(0),
        )?;
        for (article_id, phase) in &touched_screening {
            settle_screening(&transaction, project_id, article_id, phase, &mode)?;
            refresh_stage(&transaction, project_id, article_id)?;
        }

        // Quality: per-reviewer scores plus any consensus reached elsewhere.
        let mut touched_quality = BTreeSet::new();
        for row in table("quality_scores") {
            if let Some(row) = remap(row)
                && insert(&transaction, &mut columns, "quality_scores", &row)?
            {
                summary.scores_added += 1;
                touched_quality.insert(text(&row, "article_id").unwrap_or_default().to_owned());
            }
        }
        if mode == "double" && !touched_quality.is_empty() {
            let criteria = quality::quality_criteria(&transaction, project_id)?;
            for article_id in &touched_quality {
                settle_quality(&transaction, project_id, article_id, &criteria)?;
            }
        }

        // Extraction: values sit side by side; differing values open a conflict.
        let mut touched_values = BTreeSet::new();
        for row in table("extracted_values") {
            if let Some(mut row) = remap(row) {
                row.insert("extraction_run_id".into(), Value::Null);
                if insert(&transaction, &mut columns, "extracted_values", &row)? {
                    summary.values_added += 1;
                    touched_values.insert((
                        text(&row, "field_id").unwrap_or_default().to_owned(),
                        text(&row, "article_id").unwrap_or_default().to_owned(),
                    ));
                }
            }
        }
        for row in table("extraction_value_evidence") {
            insert(&transaction, &mut columns, "extraction_value_evidence", row)?;
        }
        for (field_id, article_id) in &touched_values {
            flag_extraction_conflict(&transaction, project_id, field_id, article_id)?;
        }

        summary.open_conflicts = open_conflicts(&transaction, project_id)?;
        let summary_json = serde_json::to_string(&summary).map_err(json_error)?;
        transaction.execute(
            "INSERT INTO exchange_imports (id, project_id, package_sha256, origin_reviewer_id, origin_reviewer_name, summary_json, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![Uuid::new_v4().to_string(), project_id, sha256, package.reviewer_id, package.reviewer_name, summary_json, now()],
        )?;
        audit(
            &transaction,
            project_id,
            "exchange_package",
            &sha256,
            "imported",
            &local,
            None,
            json!({ "from": package.reviewer_name, "createdAt": package.created_at, "summary": summary }),
        )?;
        transaction.commit()?;
        Ok(summary)
    }

    /// Close an extraction conflict by keeping one value; the others are rejected
    /// (not deleted) so the disagreement stays in the record.
    pub fn resolve_extraction_conflict(
        &self,
        conflict_id: &str,
        keep_value_id: &str,
        reason: &str,
    ) -> AppResult<()> {
        if reason.trim().len() < 3 {
            return Err(AppError::Validation(
                "Explica la resolución del conflicto".into(),
            ));
        }
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        let (project_id, details): (String, String) = transaction
            .query_row(
                "SELECT project_id, details_json FROM reviewer_conflicts WHERE id = ?1 AND status = 'open' AND conflict_type = 'extraction'",
                [conflict_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::Validation("El conflicto no existe o ya fue resuelto".into()))?;
        let details: Value = serde_json::from_str(&details).map_err(json_error)?;
        let value_ids: Vec<String> = details["values"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value["valueId"].as_str().map(str::to_owned))
            .collect();
        if !value_ids.iter().any(|id| id == keep_value_id) {
            return Err(AppError::Validation(
                "El valor elegido no forma parte del conflicto".into(),
            ));
        }
        let actor: Actor = active_reviewer(&transaction, &project_id)?;
        let timestamp = now();
        for value_id in &value_ids {
            let status = if value_id == keep_value_id {
                "confirmed"
            } else {
                "rejected"
            };
            transaction.execute(
                "UPDATE extracted_values SET status = ?1, notes = TRIM(notes || ' ' || ?2), updated_at = ?3 WHERE id = ?4",
                params![status, format!("{RESOLUTION_PREFIX} {}", reason.trim()), timestamp, value_id],
            )?;
        }
        transaction.execute(
            "UPDATE reviewer_conflicts SET status = 'resolved', resolution = ?1, resolver_id = ?2, resolved_at = ?3 WHERE id = ?4",
            params![reason.trim(), actor.reviewer_id, timestamp, conflict_id],
        )?;
        audit(
            &transaction,
            &project_id,
            "reviewer_conflict",
            conflict_id,
            "resolved",
            &actor,
            None,
            json!({ "kept": keep_value_id, "rejected": value_ids.iter().filter(|id| *id != keep_value_id).collect::<Vec<_>>(), "reason": reason.trim() }),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn exchange_history(&self, project_id: &str) -> AppResult<Vec<Value>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT origin_reviewer_name, package_sha256, summary_json, imported_at FROM exchange_imports
             WHERE project_id = ?1 ORDER BY imported_at DESC",
        )?;
        let rows = statement
            .query_map([project_id], |row| {
                Ok(json!({
                    "reviewerName": row.get::<_, String>(0)?,
                    "sha256": row.get::<_, String>(1)?,
                    "summary": serde_json::from_str::<Value>(&row.get::<_, String>(2)?).unwrap_or(Value::Null),
                    "importedAt": row.get::<_, String>(3)?,
                }))
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

/// After new decisions arrive: a recorded conflict resolution wins; otherwise
/// double review reconciles and single review keeps the latest decision.
fn settle_screening(
    connection: &Connection,
    project_id: &str,
    article_id: &str,
    phase: &str,
    mode: &str,
) -> AppResult<()> {
    let resolution: Option<(String, Option<String>, String)> = connection
        .query_row(
            "SELECT id, reviewer_id, reason FROM screening_decisions
             WHERE project_id = ?1 AND article_id = ?2 AND phase = ?3 AND reason LIKE ?4
             ORDER BY decided_at DESC LIMIT 1",
            params![
                project_id,
                article_id,
                phase,
                format!("{RESOLUTION_PREFIX}%")
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((decision_id, resolver, reason)) = resolution {
        connection.execute(
            "UPDATE screening_decisions SET is_final = (id = ?4) WHERE project_id = ?1 AND article_id = ?2 AND phase = ?3",
            params![project_id, article_id, phase, decision_id],
        )?;
        connection.execute(
            "UPDATE reviewer_conflicts SET status = 'resolved', resolution = ?1, resolver_id = ?2, resolved_at = ?3
             WHERE project_id = ?4 AND article_id = ?5 AND phase = ?6 AND conflict_type = 'screening' AND status = 'open'",
            params![reason.trim_start_matches(RESOLUTION_PREFIX).trim(), resolver, now(), project_id, article_id, phase],
        )?;
    } else if mode == "double" {
        screening::reconcile(connection, project_id, article_id, phase)?;
    } else {
        connection.execute(
            "UPDATE screening_decisions SET is_final = (id = (
                SELECT id FROM screening_decisions WHERE project_id = ?1 AND article_id = ?2 AND phase = ?3
                ORDER BY decided_at DESC, rowid DESC LIMIT 1))
             WHERE project_id = ?1 AND article_id = ?2 AND phase = ?3",
            params![project_id, article_id, phase],
        )?;
    }
    Ok(())
}

fn settle_quality(
    connection: &Connection,
    project_id: &str,
    article_id: &str,
    criteria: &[quality::QualityCriterion],
) -> AppResult<()> {
    let resolver: Option<String> = connection
        .query_row(
            "SELECT s.reviewer_id FROM quality_scores s JOIN quality_criteria c ON c.id = s.criterion_id
             WHERE c.project_id = ?1 AND s.article_id = ?2 AND s.actor_name = ?3 AND s.reviewer_id IS NOT NULL
             ORDER BY s.assessed_at DESC LIMIT 1",
            params![project_id, article_id, CONSENSUS],
            |row| row.get(0),
        )
        .optional()?;
    match resolver {
        Some(resolver) => {
            connection.execute(
                "UPDATE reviewer_conflicts SET status = 'resolved', resolution = 'Consenso registrado por otro revisor (paquete de intercambio)',
                        resolver_id = ?1, resolved_at = ?2
                 WHERE project_id = ?3 AND article_id = ?4 AND conflict_type = 'quality' AND status = 'open'",
                params![resolver, now(), project_id, article_id],
            )?;
        }
        None => quality::reconcile(connection, project_id, article_id, criteria)?,
    }
    Ok(())
}

/// Open (or refresh) an extraction conflict when reviewers recorded different
/// values for the same field and study.
fn flag_extraction_conflict(
    connection: &Connection,
    project_id: &str,
    field_id: &str,
    article_id: &str,
) -> AppResult<()> {
    let mut statement = connection.prepare(
        "SELECT v.id, v.normalized_value_json, v.actor_name, v.reviewer_id, f.name FROM extracted_values v
         JOIN extraction_fields f ON f.id = v.field_id
         WHERE v.field_id = ?1 AND v.article_id = ?2 AND v.actor_type = 'human' AND v.reviewer_id IS NOT NULL
           AND v.status NOT IN ('rejected', 'proposed') ORDER BY v.created_at",
    )?;
    let values: Vec<(String, String, String, String, String)> = statement
        .query_map(params![field_id, article_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let reviewers: HashSet<&str> = values.iter().map(|value| value.3.as_str()).collect();
    let distinct: HashSet<&str> = values.iter().map(|value| value.1.as_str()).collect();
    if reviewers.len() < 2 || distinct.len() < 2 {
        return Ok(());
    }
    let details = json!({
        "fieldId": field_id,
        "fieldName": values[0].4,
        "values": values.iter().map(|(id, value, name, _, _)| json!({
            "valueId": id,
            "reviewer": name,
            "value": serde_json::from_str::<Value>(value).unwrap_or(Value::Null),
        })).collect::<Vec<_>>(),
    });
    let open: Option<String> = connection
        .query_row(
            "SELECT id FROM reviewer_conflicts WHERE project_id = ?1 AND article_id = ?2 AND conflict_type = 'extraction'
               AND status = 'open' AND json_extract(details_json, '$.fieldId') = ?3",
            params![project_id, article_id, field_id],
            |row| row.get(0),
        )
        .optional()?;
    match open {
        Some(id) => {
            connection.execute(
                "UPDATE reviewer_conflicts SET details_json = ?1 WHERE id = ?2",
                params![details.to_string(), id],
            )?;
        }
        None => {
            connection.execute(
                "INSERT INTO reviewer_conflicts (id, project_id, article_id, conflict_type, status, created_at, details_json)
                 VALUES (?1, ?2, ?3, 'extraction', 'open', ?4, ?5)",
                params![Uuid::new_v4().to_string(), project_id, article_id, now(), details.to_string()],
            )?;
        }
    }
    Ok(())
}

fn match_article(connection: &Connection, article: &Record) -> AppResult<Option<String>> {
    let queries: [(&str, Vec<SqlValue>); 4] = [
        (
            "SELECT id FROM articles WHERE id = ?1",
            vec![sql_text(article, "id")],
        ),
        (
            "SELECT id FROM articles WHERE ?1 IS NOT NULL AND lower(doi) = lower(?1) AND deleted_at IS NULL",
            vec![sql_text(article, "doi")],
        ),
        (
            "SELECT id FROM articles WHERE ?1 IS NOT NULL AND document_sha256 = ?1 AND deleted_at IS NULL",
            vec![sql_text(article, "document_sha256")],
        ),
        (
            "SELECT id FROM articles WHERE lower(trim(title)) = lower(trim(?1)) AND publication_year IS ?2 AND deleted_at IS NULL",
            vec![
                sql_text(article, "title"),
                to_sql(article.get("publication_year")),
            ],
        ),
    ];
    for (sql, values) in queries {
        let found: Option<String> = connection
            .query_row(sql, params_from_iter(values.iter()), |row| row.get(0))
            .optional()?;
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(None)
}

fn evidence_exists(connection: &Connection, row: &Record) -> AppResult<bool> {
    match text(row, "evidence_id") {
        None => Ok(true),
        Some(id) => Ok(connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM evidence_spans WHERE id = ?1)",
            [id],
            |r| r.get(0),
        )?),
    }
}

fn open_conflicts(connection: &Connection, project_id: &str) -> AppResult<i64> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM reviewer_conflicts WHERE project_id = ?1 AND status = 'open'",
        [project_id],
        |row| row.get(0),
    )?)
}

/// Columns that exist locally, per table. Package keys are only ever used as
/// column names after passing through this allow-list.
#[derive(Default)]
struct ColumnCache(HashMap<String, HashSet<String>>);

impl ColumnCache {
    fn columns(&mut self, connection: &Connection, table: &str) -> AppResult<&HashSet<String>> {
        if !self.0.contains_key(table) {
            let mut statement = connection.prepare("SELECT name FROM pragma_table_info(?1)")?;
            let names = statement
                .query_map([table], |row| row.get::<_, String>(0))?
                .collect::<Result<HashSet<_>, _>>()?;
            self.0.insert(table.to_owned(), names);
        }
        Ok(&self.0[table])
    }
}

/// `INSERT OR IGNORE` one record; true when a row was added.
fn insert(
    connection: &Connection,
    cache: &mut ColumnCache,
    table: &str,
    record: &Record,
) -> AppResult<bool> {
    let known = cache.columns(connection, table)?;
    let (names, values): (Vec<&String>, Vec<SqlValue>) = record
        .iter()
        .filter(|(name, _)| known.contains(name.as_str()))
        .map(|(name, value)| (name, to_sql(Some(value))))
        .unzip();
    if names.is_empty() {
        return Ok(false);
    }
    let sql = format!(
        "INSERT OR IGNORE INTO {table} ({}) VALUES ({})",
        names
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", "),
        (1..=names.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    Ok(connection.execute(&sql, params_from_iter(values.iter()))? > 0)
}

fn select_records(
    connection: &Connection,
    sql: &str,
    args: &[&dyn ToSql],
) -> AppResult<Vec<Record>> {
    let mut statement = connection.prepare(sql)?;
    let names: Vec<String> = statement.column_names().map(str::to_owned).collect();
    let mut rows = statement.query(args)?;
    let mut records = Vec::new();
    while let Some(row) = rows.next()? {
        let mut record = Map::new();
        for (index, name) in names.iter().enumerate() {
            let value = match row.get_ref(index)? {
                ValueRef::Null | ValueRef::Blob(_) => Value::Null,
                ValueRef::Integer(number) => json!(number),
                ValueRef::Real(number) => json!(number),
                ValueRef::Text(bytes) => json!(String::from_utf8_lossy(bytes)),
            };
            record.insert(name.clone(), value);
        }
        records.push(record);
    }
    Ok(records)
}

fn to_sql(value: Option<&Value>) -> SqlValue {
    match value {
        None | Some(Value::Null) => SqlValue::Null,
        Some(Value::Bool(flag)) => SqlValue::Integer(i64::from(*flag)),
        Some(Value::Number(number)) => number
            .as_i64()
            .map(SqlValue::Integer)
            .or_else(|| number.as_f64().map(SqlValue::Real))
            .unwrap_or(SqlValue::Null),
        Some(Value::String(text)) => SqlValue::Text(text.clone()),
        Some(other) => SqlValue::Text(other.to_string()),
    }
}

fn sql_text(record: &Record, key: &str) -> SqlValue {
    text(record, key).map_or(SqlValue::Null, |value| SqlValue::Text(value.to_owned()))
}

fn text<'a>(record: &'a Record, key: &str) -> Option<&'a str> {
    record
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn json_error(error: serde_json::Error) -> AppError {
    AppError::Initialization(error.to_string())
}

#[cfg(test)]
#[path = "exchange_tests.rs"]
mod tests;
