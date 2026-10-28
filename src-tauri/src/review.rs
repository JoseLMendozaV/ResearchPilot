//! Systematic-review workspace: project settings, reviewers, versioned
//! protocol, project corpus, control articles, research questions with typed
//! aspects, and search strategies (sheets Leer, Control, RQs and Cadena of the
//! reference evidence workbook). Every change is audited per project.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    db::{Database, now},
    error::{AppError, AppResult},
};

fn to_json(value: &impl Serialize) -> AppResult<String> {
    serde_json::to_string(value).map_err(|error| AppError::Validation(error.to_string()))
}

fn require(condition: bool, message: &str) -> AppResult<()> {
    if condition {
        Ok(())
    } else {
        Err(AppError::Validation(message.into()))
    }
}

pub(crate) struct Actor {
    pub reviewer_id: String,
    pub name: String,
}

/// The local profile, created on first use and registered as project owner.
pub(crate) fn local_reviewer(connection: &Connection, project_id: &str) -> AppResult<Actor> {
    let researcher: String = connection.query_row(
        "SELECT researcher_name FROM app_settings WHERE id = 1",
        [],
        |row| row.get(0),
    )?;
    let name = if researcher.trim().is_empty() {
        "Investigador local".to_owned()
    } else {
        researcher.trim().to_owned()
    };
    let existing: Option<String> = connection
        .query_row(
            "SELECT id FROM reviewers WHERE is_local_profile = 1 LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let reviewer_id = match existing {
        Some(id) => {
            connection.execute(
                "UPDATE reviewers SET display_name = ?1 WHERE id = ?2",
                params![name, id],
            )?;
            id
        }
        None => {
            let id = Uuid::new_v4().to_string();
            connection.execute(
                "INSERT INTO reviewers (id, display_name, is_local_profile, created_at) VALUES (?1, ?2, 1, ?3)",
                params![id, name, now()],
            )?;
            id
        }
    };
    connection.execute(
        "INSERT OR IGNORE INTO project_reviewers (project_id, reviewer_id, role) VALUES (?1, ?2, 'owner')",
        params![project_id, reviewer_id],
    )?;
    Ok(Actor { reviewer_id, name })
}

/// Reviewer whose decisions the local user is currently recording.
pub(crate) fn active_reviewer(connection: &Connection, project_id: &str) -> AppResult<Actor> {
    let local = local_reviewer(connection, project_id)?;
    let active: Option<(String, String)> = connection
        .query_row(
            "SELECT r.id, r.display_name FROM review_projects p
             JOIN project_reviewers pr ON pr.project_id = p.id AND pr.reviewer_id = p.active_reviewer_id
             JOIN reviewers r ON r.id = pr.reviewer_id WHERE p.id = ?1",
            [project_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(active.map_or(local, |(reviewer_id, name)| Actor { reviewer_id, name }))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn audit(
    connection: &Connection,
    project_id: &str,
    entity_type: &str,
    entity_id: &str,
    action: &str,
    actor: &Actor,
    before: Option<Value>,
    after: Value,
) -> AppResult<()> {
    connection.execute(
        "INSERT INTO audit_events (id, project_id, entity_type, entity_id, action, actor_type, actor_name, before_json, after_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'human', ?6, ?7, ?8, ?9)",
        params![Uuid::new_v4().to_string(), project_id, entity_type, entity_id, action, actor.name, before.map(|value| value.to_string()), after.to_string(), now()],
    )?;
    Ok(())
}

pub(crate) fn ensure_project(connection: &Connection, project_id: &str) -> AppResult<()> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM review_projects WHERE id = ?1 AND deleted_at IS NULL)",
        [project_id],
        |row| row.get(0),
    )?;
    require(exists, "La revisión no existe")
}

// --- Workspace ----------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetail {
    pub id: String,
    pub name: String,
    pub description: String,
    pub review_type: String,
    pub status: String,
    pub screening_mode: String,
    pub quality_threshold: Option<f64>,
    pub quality_missing_policy: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewerInfo {
    pub id: String,
    pub display_name: String,
    pub role: String,
    pub is_local_profile: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Amendment {
    pub reason: String,
    pub change: Value,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolVersion {
    pub id: String,
    pub version_number: i64,
    pub status: String,
    pub content: Value,
    pub change_summary: String,
    pub created_by: String,
    pub approved_at: Option<String>,
    pub created_at: String,
    pub amendments: Vec<Amendment>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StageCounts {
    pub total: i64,
    pub identified: i64,
    pub screening: i64,
    pub eligible: i64,
    pub included: i64,
    pub excluded: i64,
    pub secondary: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorkspace {
    pub project: ProjectDetail,
    pub reviewers: Vec<ReviewerInfo>,
    pub active_reviewer_id: String,
    pub protocol: Vec<ProtocolVersion>,
    pub counts: StageCounts,
    pub open_conflicts: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettingsInput {
    pub id: String,
    pub name: String,
    pub description: String,
    pub review_type: String,
    pub status: String,
    pub screening_mode: String,
    pub quality_threshold: Option<f64>,
    pub quality_missing_policy: String,
}

impl Database {
    pub fn project_workspace(&self, project_id: &str) -> AppResult<ProjectWorkspace> {
        let connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let active = active_reviewer(&connection, project_id)?;
        let project = connection.query_row(
            "SELECT id, name, description, review_type, status, screening_mode, quality_threshold, quality_missing_policy, created_at, updated_at
             FROM review_projects WHERE id = ?1",
            [project_id],
            |row| {
                Ok(ProjectDetail {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    review_type: row.get(3)?,
                    status: row.get(4)?,
                    screening_mode: row.get(5)?,
                    quality_threshold: row.get(6)?,
                    quality_missing_policy: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        )?;
        let reviewers = {
            let mut statement = connection.prepare(
                "SELECT r.id, r.display_name, pr.role, r.is_local_profile FROM project_reviewers pr
                 JOIN reviewers r ON r.id = pr.reviewer_id WHERE pr.project_id = ?1 ORDER BY r.is_local_profile DESC, r.display_name",
            )?;
            statement
                .query_map([project_id], |row| {
                    Ok(ReviewerInfo {
                        id: row.get(0)?,
                        display_name: row.get(1)?,
                        role: row.get(2)?,
                        is_local_profile: row.get(3)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let counts = connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(workflow_stage = 'identified'), 0), COALESCE(SUM(workflow_stage = 'screening'), 0),
                    COALESCE(SUM(workflow_stage = 'eligible'), 0), COALESCE(SUM(workflow_stage = 'included'), 0),
                    COALESCE(SUM(workflow_stage = 'excluded'), 0), COALESCE(SUM(is_secondary), 0)
             FROM review_articles WHERE project_id = ?1 AND removed_at IS NULL",
            [project_id],
            |row| {
                Ok(StageCounts {
                    total: row.get(0)?,
                    identified: row.get(1)?,
                    screening: row.get(2)?,
                    eligible: row.get(3)?,
                    included: row.get(4)?,
                    excluded: row.get(5)?,
                    secondary: row.get(6)?,
                })
            },
        )?;
        let open_conflicts = connection.query_row(
            "SELECT COUNT(*) FROM reviewer_conflicts WHERE project_id = ?1 AND status = 'open'",
            [project_id],
            |row| row.get(0),
        )?;
        Ok(ProjectWorkspace {
            project,
            reviewers,
            active_reviewer_id: active.reviewer_id,
            protocol: protocol_versions(&connection, project_id)?,
            counts,
            open_conflicts,
        })
    }

    pub fn update_project_settings(
        &self,
        input: ProjectSettingsInput,
    ) -> AppResult<ProjectWorkspace> {
        let name = input.name.trim();
        require(
            (3..=140).contains(&name.chars().count()),
            "El nombre debe tener entre 3 y 140 caracteres",
        )?;
        require(
            ["systematic_review", "systematic_mapping", "scoping_review"]
                .contains(&input.review_type.as_str()),
            "Tipo de revisión no reconocido",
        )?;
        require(
            ["draft", "active", "completed", "archived"].contains(&input.status.as_str()),
            "Estado no reconocido",
        )?;
        require(
            ["single", "double"].contains(&input.screening_mode.as_str()),
            "Modo de revisión no reconocido",
        )?;
        require(
            ["zero", "exclude_from_max"].contains(&input.quality_missing_policy.as_str()),
            "Regla de faltantes no reconocida",
        )?;
        require(
            input
                .quality_threshold
                .is_none_or(|value| (0.0..=100.0).contains(&value)),
            "El umbral de calidad debe estar entre 0 y 100 %",
        )?;
        let connection = self.open()?;
        ensure_project(&connection, &input.id)?;
        let actor = active_reviewer(&connection, &input.id)?;
        let before = to_json(&self.project_workspace(&input.id)?.project)?;
        connection.execute(
            "UPDATE review_projects SET name = ?1, description = ?2, review_type = ?3, status = ?4, screening_mode = ?5,
                    quality_threshold = ?6, quality_missing_policy = ?7, updated_at = ?8 WHERE id = ?9",
            params![name, input.description.trim(), input.review_type, input.status, input.screening_mode, input.quality_threshold, input.quality_missing_policy, now(), input.id],
        )?;
        audit(
            &connection,
            &input.id,
            "review_project",
            &input.id,
            "settings_updated",
            &actor,
            serde_json::from_str(&before).ok(),
            json!({ "name": name, "screeningMode": input.screening_mode, "status": input.status }),
        )?;
        self.project_workspace(&input.id)
    }

    pub fn add_project_reviewer(
        &self,
        project_id: &str,
        name: &str,
        role: &str,
    ) -> AppResult<ProjectWorkspace> {
        let name = name.trim();
        require(
            (2..=80).contains(&name.chars().count()),
            "El nombre del revisor debe tener entre 2 y 80 caracteres",
        )?;
        require(
            ["reviewer", "resolver", "owner"].contains(&role),
            "Rol no reconocido",
        )?;
        let connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let actor = active_reviewer(&connection, project_id)?;
        let reviewer_id = Uuid::new_v4().to_string();
        connection.execute("INSERT INTO reviewers (id, display_name, is_local_profile, created_at) VALUES (?1, ?2, 0, ?3)", params![reviewer_id, name, now()])?;
        connection.execute(
            "INSERT INTO project_reviewers (project_id, reviewer_id, role) VALUES (?1, ?2, ?3)",
            params![project_id, reviewer_id, role],
        )?;
        audit(
            &connection,
            project_id,
            "reviewer",
            &reviewer_id,
            "added",
            &actor,
            None,
            json!({ "name": name, "role": role }),
        )?;
        self.project_workspace(project_id)
    }

    /// Choose whose decisions are being recorded (e.g. transcribing a second reviewer's sheet).
    pub fn set_active_reviewer(
        &self,
        project_id: &str,
        reviewer_id: &str,
    ) -> AppResult<ProjectWorkspace> {
        let connection = self.open()?;
        let member: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM project_reviewers WHERE project_id = ?1 AND reviewer_id = ?2)",
            params![project_id, reviewer_id],
            |row| row.get(0),
        )?;
        require(member, "El revisor no pertenece a esta revisión")?;
        connection.execute(
            "UPDATE review_projects SET active_reviewer_id = ?1 WHERE id = ?2",
            params![reviewer_id, project_id],
        )?;
        self.project_workspace(project_id)
    }

    // --- Protocol ---------------------------------------------------------------------

    /// Edit the draft. If the latest version is approved, a new draft version is
    /// created and the change must carry an amendment reason.
    pub fn save_protocol(
        &self,
        project_id: &str,
        content: Value,
        amendment_reason: Option<String>,
    ) -> AppResult<Vec<ProtocolVersion>> {
        require(content.is_object(), "El protocolo debe ser un objeto")?;
        let mut connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let transaction = connection.transaction()?;
        let actor = active_reviewer(&transaction, project_id)?;
        let latest: Option<(String, i64, String, String)> = transaction
            .query_row(
                "SELECT id, version_number, status, content_json FROM protocol_versions WHERE project_id = ?1 ORDER BY version_number DESC LIMIT 1",
                [project_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        match latest {
            Some((id, _, status, before)) if status == "draft" => {
                transaction.execute(
                    "UPDATE protocol_versions SET content_json = ?1 WHERE id = ?2",
                    params![content.to_string(), id],
                )?;
                audit(
                    &transaction,
                    project_id,
                    "protocol_version",
                    &id,
                    "draft_updated",
                    &actor,
                    serde_json::from_str(&before).ok(),
                    content,
                )?;
            }
            Some((_, number, _, before)) => {
                let reason = amendment_reason.map(|reason| reason.trim().to_owned()).filter(|reason| reason.len() >= 5).ok_or_else(|| {
                    AppError::Validation("El protocolo aprobado solo cambia mediante una enmienda con motivo (mínimo 5 caracteres)".into())
                })?;
                let id = Uuid::new_v4().to_string();
                transaction.execute(
                    "INSERT INTO protocol_versions (id, project_id, version_number, content_json, change_summary, status, created_by, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'draft', ?6, ?7)",
                    params![id, project_id, number + 1, content.to_string(), reason, actor.name, now()],
                )?;
                let before: Value = serde_json::from_str(&before).unwrap_or(Value::Null);
                let changed: Vec<&String> = content
                    .as_object()
                    .map(|object| {
                        object
                            .keys()
                            .filter(|key| before.get(*key) != content.get(*key))
                            .collect()
                    })
                    .unwrap_or_default();
                transaction.execute(
                    "INSERT INTO protocol_amendments (id, protocol_version_id, reason, change_json, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![Uuid::new_v4().to_string(), id, reason, json!({ "changedSections": changed }).to_string(), actor.name, now()],
                )?;
                audit(
                    &transaction,
                    project_id,
                    "protocol_version",
                    &id,
                    "amendment_created",
                    &actor,
                    Some(before),
                    json!({ "reason": reason, "version": number + 1 }),
                )?;
            }
            None => {
                let id = Uuid::new_v4().to_string();
                transaction.execute(
                    "INSERT INTO protocol_versions (id, project_id, version_number, content_json, change_summary, status, created_by, created_at)
                     VALUES (?1, ?2, 1, ?3, 'Versión inicial', 'draft', ?4, ?5)",
                    params![id, project_id, content.to_string(), actor.name, now()],
                )?;
                audit(
                    &transaction,
                    project_id,
                    "protocol_version",
                    &id,
                    "created",
                    &actor,
                    None,
                    content,
                )?;
            }
        }
        transaction.commit()?;
        protocol_versions(&self.open()?, project_id)
    }

    /// Approve the draft, freezing a snapshot of RQs, aspects and criteria.
    pub fn approve_protocol(&self, project_id: &str) -> AppResult<Vec<ProtocolVersion>> {
        let mut connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let transaction = connection.transaction()?;
        let actor = active_reviewer(&transaction, project_id)?;
        let (id, content): (String, String) = transaction
            .query_row(
                "SELECT id, content_json FROM protocol_versions WHERE project_id = ?1 AND status = 'draft' ORDER BY version_number DESC LIMIT 1",
                [project_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::Validation("No hay un borrador de protocolo para aprobar".into()))?;
        let mut content: Value = serde_json::from_str(&content).unwrap_or(json!({}));
        content["snapshot"] = protocol_snapshot(&transaction, project_id)?;
        transaction.execute("UPDATE protocol_versions SET status = 'superseded' WHERE project_id = ?1 AND status = 'approved'", [project_id])?;
        transaction.execute(
            "UPDATE protocol_versions SET status = 'approved', approved_at = ?1, content_json = ?2 WHERE id = ?3",
            params![now(), content.to_string(), id],
        )?;
        audit(
            &transaction,
            project_id,
            "protocol_version",
            &id,
            "approved",
            &actor,
            None,
            json!({ "snapshot": content["snapshot"] }),
        )?;
        transaction.commit()?;
        protocol_versions(&self.open()?, project_id)
    }
}

fn protocol_snapshot(connection: &Connection, project_id: &str) -> AppResult<Value> {
    let rows = |sql: &str| -> AppResult<Vec<Value>> {
        let mut statement = connection.prepare(sql)?;
        let values = statement
            .query_map([project_id], |row| {
                let mut object = serde_json::Map::new();
                for index in 0..row.as_ref().column_count() {
                    let name = row.as_ref().column_name(index).unwrap_or("").to_owned();
                    let value: rusqlite::types::Value = row.get(index)?;
                    object.insert(
                        name,
                        match value {
                            rusqlite::types::Value::Null => Value::Null,
                            rusqlite::types::Value::Integer(number) => json!(number),
                            rusqlite::types::Value::Real(number) => json!(number),
                            rusqlite::types::Value::Text(text) => json!(text),
                            rusqlite::types::Value::Blob(_) => Value::Null,
                        },
                    );
                }
                Ok(Value::Object(object))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(values)
    };
    Ok(json!({
        "researchQuestions": rows("SELECT code, question, objective, category FROM research_questions WHERE project_id = ?1 ORDER BY display_order, code")?,
        "extractionFields": rows("SELECT f.name, f.data_type, f.required, q.code AS rq FROM extraction_fields f LEFT JOIN research_questions q ON q.id = f.research_question_id WHERE f.project_id = ?1 AND f.deleted_at IS NULL ORDER BY f.display_order")?,
        "eligibilityCriteria": rows("SELECT criterion_type, code, name, description FROM eligibility_criteria WHERE project_id = ?1 AND active = 1 ORDER BY CASE criterion_type WHEN 'inclusion' THEN 0 ELSE 1 END, display_order")?,
        "qualityCriteria": rows("SELECT code, question, weight, max_score, scale_json FROM quality_criteria WHERE project_id = ?1 ORDER BY display_order")?,
        "searchStrategies": rows("SELECT database_name, query_text, query_date, result_count, version_number FROM search_strategies WHERE project_id = ?1 AND deleted_at IS NULL ORDER BY database_name, version_number")?,
    }))
}

fn protocol_versions(connection: &Connection, project_id: &str) -> AppResult<Vec<ProtocolVersion>> {
    let mut statement = connection.prepare(
        "SELECT id, version_number, status, content_json, change_summary, created_by, approved_at, created_at
         FROM protocol_versions WHERE project_id = ?1 ORDER BY version_number DESC",
    )?;
    let mut versions = statement
        .query_map([project_id], |row| {
            Ok(ProtocolVersion {
                id: row.get(0)?,
                version_number: row.get(1)?,
                status: row.get(2)?,
                content: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or(Value::Null),
                change_summary: row.get(4)?,
                created_by: row.get(5)?,
                approved_at: row.get(6)?,
                created_at: row.get(7)?,
                amendments: Vec::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut amendments = connection.prepare("SELECT reason, change_json, created_by, created_at FROM protocol_amendments WHERE protocol_version_id = ?1 ORDER BY created_at")?;
    for version in &mut versions {
        version.amendments = amendments
            .query_map([&version.id], |row| {
                Ok(Amendment {
                    reason: row.get(0)?,
                    change: serde_json::from_str(&row.get::<_, String>(1)?).unwrap_or(Value::Null),
                    created_by: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    Ok(versions)
}

// --- Corpus ---------------------------------------------------------------------------

impl Database {
    /// Add library articles to the project corpus with a stable sequential Id.
    pub fn add_to_corpus(
        &self,
        project_id: &str,
        article_ids: &[String],
        source_database: Option<&str>,
    ) -> AppResult<i64> {
        let mut connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let transaction = connection.transaction()?;
        let actor = active_reviewer(&transaction, project_id)?;
        let added = add_articles(&transaction, project_id, article_ids, source_database)?;
        if added > 0 {
            audit(
                &transaction,
                project_id,
                "corpus",
                project_id,
                "articles_added",
                &actor,
                None,
                json!({ "count": added, "source": source_database }),
            )?;
        }
        transaction.commit()?;
        Ok(added)
    }

    pub fn set_corpus_flags(
        &self,
        project_id: &str,
        article_id: &str,
        is_secondary: Option<bool>,
        removed: Option<bool>,
        notes: Option<String>,
    ) -> AppResult<()> {
        let connection = self.open()?;
        let actor = active_reviewer(&connection, project_id)?;
        let changed = connection.execute(
            "UPDATE review_articles SET is_secondary = COALESCE(?1, is_secondary),
                    removed_at = CASE WHEN ?2 IS NULL THEN removed_at WHEN ?2 = 1 THEN COALESCE(removed_at, ?3) ELSE NULL END,
                    notes = COALESCE(?4, notes)
             WHERE project_id = ?5 AND article_id = ?6",
            params![is_secondary, removed, now(), notes, project_id, article_id],
        )?;
        require(changed > 0, "El artículo no forma parte de esta revisión")?;
        audit(
            &connection,
            project_id,
            "corpus_article",
            article_id,
            "flags_updated",
            &actor,
            None,
            json!({ "secondary": is_secondary, "removed": removed }),
        )
    }
}

pub(crate) fn add_articles(
    connection: &Connection,
    project_id: &str,
    article_ids: &[String],
    source_database: Option<&str>,
) -> AppResult<i64> {
    let mut next: i64 = connection.query_row(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM review_articles WHERE project_id = ?1",
        [project_id],
        |row| row.get(0),
    )?;
    let mut added = 0;
    for article_id in article_ids {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM articles WHERE id = ?1 AND deleted_at IS NULL)",
            [article_id],
            |row| row.get(0),
        )?;
        if !exists {
            continue;
        }
        let inserted = connection.execute(
            "INSERT OR IGNORE INTO review_articles (project_id, article_id, workflow_stage, source_database, imported_at, sequence)
             VALUES (?1, ?2, 'identified', ?3, ?4, ?5)",
            params![project_id, article_id, source_database, now(), next],
        )?;
        if inserted > 0 {
            next += 1;
            added += 1;
        } else {
            connection.execute("UPDATE review_articles SET removed_at = NULL WHERE project_id = ?1 AND article_id = ?2", params![project_id, article_id])?;
        }
    }
    Ok(added)
}

// --- Control articles ---------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlArticle {
    pub id: String,
    pub article_id: String,
    pub title: String,
    pub doi: Option<String>,
    pub year: Option<i64>,
    pub rationale: String,
    pub expected_relevant: bool,
    pub in_corpus: bool,
    /// Search strategies (database + version) whose imported results contain this article.
    pub found_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlCoverage {
    pub articles: Vec<ControlArticle>,
    pub found: i64,
    pub total: i64,
}

impl Database {
    pub fn add_control_article(
        &self,
        project_id: &str,
        article_id: &str,
        rationale: &str,
    ) -> AppResult<ControlCoverage> {
        let connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let actor = active_reviewer(&connection, project_id)?;
        let order: i64 = connection.query_row("SELECT COALESCE(MAX(display_order), 0) + 1 FROM control_articles WHERE project_id = ?1", [project_id], |row| row.get(0))?;
        let id = Uuid::new_v4().to_string();
        let inserted = connection.execute(
            "INSERT OR IGNORE INTO control_articles (id, project_id, article_id, rationale, display_order, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, project_id, article_id, rationale.trim(), order, now()],
        )?;
        require(inserted > 0, "Ese artículo ya es un artículo de control")?;
        audit(
            &connection,
            project_id,
            "control_article",
            &id,
            "added",
            &actor,
            None,
            json!({ "articleId": article_id, "rationale": rationale.trim() }),
        )?;
        self.control_coverage(project_id)
    }

    pub fn remove_control_article(
        &self,
        project_id: &str,
        control_id: &str,
    ) -> AppResult<ControlCoverage> {
        let connection = self.open()?;
        let actor = active_reviewer(&connection, project_id)?;
        let changed = connection.execute(
            "DELETE FROM control_articles WHERE id = ?1 AND project_id = ?2",
            params![control_id, project_id],
        )?;
        require(changed > 0, "El artículo de control no existe")?;
        audit(
            &connection,
            project_id,
            "control_article",
            control_id,
            "removed",
            &actor,
            None,
            json!({}),
        )?;
        self.control_coverage(project_id)
    }

    /// Coverage counts only control articles retrieved by an imported search, not added by hand.
    pub fn control_coverage(&self, project_id: &str) -> AppResult<ControlCoverage> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT c.id, a.id, a.title, a.doi, a.publication_year, c.rationale, c.expected_relevant,
                    EXISTS(SELECT 1 FROM review_articles r WHERE r.project_id = c.project_id AND r.article_id = a.id AND r.removed_at IS NULL),
                    COALESCE((SELECT json_group_array(DISTINCT s.database_name || ' v' || s.version_number)
                              FROM search_imports si
                              JOIN search_strategies s ON s.id = si.search_strategy_id AND s.project_id = c.project_id AND s.deleted_at IS NULL
                              JOIN source_records sr ON sr.import_batch_id = si.import_batch_id AND sr.article_id = a.id), '[]')
             FROM control_articles c JOIN articles a ON a.id = c.article_id
             WHERE c.project_id = ?1 ORDER BY c.display_order",
        )?;
        let articles: Vec<ControlArticle> = statement
            .query_map([project_id], |row| {
                Ok(ControlArticle {
                    id: row.get(0)?,
                    article_id: row.get(1)?,
                    title: row.get(2)?,
                    doi: row.get(3)?,
                    year: row.get(4)?,
                    rationale: row.get(5)?,
                    expected_relevant: row.get(6)?,
                    in_corpus: row.get(7)?,
                    found_by: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default(),
                })
            })?
            .collect::<Result<_, _>>()?;
        let found = articles
            .iter()
            .filter(|article| !article.found_by.is_empty())
            .count() as i64;
        Ok(ControlCoverage {
            total: articles.len() as i64,
            found,
            articles,
        })
    }
}

// --- Research questions and aspects -------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionField {
    pub id: String,
    pub research_question_id: Option<String>,
    pub name: String,
    pub description: String,
    pub data_type: String,
    pub unit: Option<String>,
    pub taxonomy: Vec<String>,
    pub instructions: String,
    pub required: bool,
    pub examples: Vec<String>,
    pub master_field_key: Option<String>,
    pub display_order: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchQuestion {
    pub id: String,
    pub parent_id: Option<String>,
    pub code: String,
    pub question: String,
    pub objective: String,
    pub category: String,
    pub display_order: i64,
    pub fields: Vec<ExtractionField>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionsOverview {
    pub questions: Vec<ResearchQuestion>,
    /// Aspects outside any RQ (the "General" block of the extraction sheet).
    pub general_fields: Vec<ExtractionField>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionInput {
    pub id: Option<String>,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub code: Option<String>,
    pub question: String,
    pub objective: String,
    pub category: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInput {
    pub id: Option<String>,
    pub project_id: String,
    pub research_question_id: Option<String>,
    pub name: String,
    pub description: String,
    pub data_type: String,
    pub unit: Option<String>,
    pub taxonomy: Vec<String>,
    pub instructions: String,
    pub required: bool,
    pub examples: Vec<String>,
    pub master_field_key: Option<String>,
}

const FIELD_TYPES: [&str; 6] = ["text", "number", "boolean", "date", "category", "list"];

impl Database {
    pub fn research_questions(&self, project_id: &str) -> AppResult<QuestionsOverview> {
        let connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let mut fields_statement = connection.prepare(
            "SELECT id, research_question_id, name, description, data_type, unit, taxonomy_json, instructions, required, examples_json, master_field_key, display_order
             FROM extraction_fields WHERE project_id = ?1 AND deleted_at IS NULL ORDER BY display_order, name",
        )?;
        let fields: Vec<ExtractionField> = fields_statement
            .query_map([project_id], |row| {
                Ok(ExtractionField {
                    id: row.get(0)?,
                    research_question_id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    data_type: row.get(4)?,
                    unit: row.get(5)?,
                    taxonomy: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                    instructions: row.get(7)?,
                    required: row.get(8)?,
                    examples: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
                    master_field_key: row.get(10)?,
                    display_order: row.get(11)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        let mut statement = connection.prepare(
            "SELECT id, parent_id, code, question, objective, category, display_order FROM research_questions WHERE project_id = ?1 ORDER BY display_order, code",
        )?;
        let questions = statement
            .query_map([project_id], |row| {
                Ok(ResearchQuestion {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    code: row.get(2)?,
                    question: row.get(3)?,
                    objective: row.get(4)?,
                    category: row.get(5)?,
                    display_order: row.get(6)?,
                    fields: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|mut question| {
                question.fields = fields
                    .iter()
                    .filter(|field| {
                        field.research_question_id.as_deref() == Some(question.id.as_str())
                    })
                    .cloned()
                    .collect();
                question
            })
            .collect();
        Ok(QuestionsOverview {
            questions,
            general_fields: fields
                .into_iter()
                .filter(|field| field.research_question_id.is_none())
                .collect(),
        })
    }

    pub fn save_research_question(&self, input: QuestionInput) -> AppResult<QuestionsOverview> {
        let question = input.question.trim();
        require(
            (5..=1000).contains(&question.chars().count()),
            "La pregunta debe tener entre 5 y 1000 caracteres",
        )?;
        let connection = self.open()?;
        ensure_project(&connection, &input.project_id)?;
        let actor = active_reviewer(&connection, &input.project_id)?;
        let code = match input
            .code
            .as_deref()
            .map(str::trim)
            .filter(|code| !code.is_empty())
        {
            Some(code) => code.to_owned(),
            None => {
                let count: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM research_questions WHERE project_id = ?1",
                    [&input.project_id],
                    |row| row.get(0),
                )?;
                let mut number = count + 1;
                loop {
                    let candidate = format!("RQ{number}");
                    let taken: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM research_questions WHERE project_id = ?1 AND code = ?2)", params![input.project_id, candidate], |row| row.get(0))?;
                    if !taken {
                        break candidate;
                    }
                    number += 1;
                }
            }
        };
        require(
            code.chars().count() <= 20,
            "El código de la RQ es demasiado largo",
        )?;
        let timestamp = now();
        match &input.id {
            Some(id) => {
                let changed = connection.execute(
                    "UPDATE research_questions SET code = ?1, question = ?2, objective = ?3, category = ?4, parent_id = ?5, updated_at = ?6 WHERE id = ?7 AND project_id = ?8",
                    params![code, question, input.objective.trim(), input.category.trim(), input.parent_id, timestamp, id, input.project_id],
                )?;
                require(changed > 0, "La pregunta no existe")?;
                audit(
                    &connection,
                    &input.project_id,
                    "research_question",
                    id,
                    "updated",
                    &actor,
                    None,
                    json!({ "code": code, "question": question }),
                )?;
            }
            None => {
                let id = Uuid::new_v4().to_string();
                let order: i64 = connection.query_row("SELECT COALESCE(MAX(display_order), 0) + 1 FROM research_questions WHERE project_id = ?1", [&input.project_id], |row| row.get(0))?;
                connection
                    .execute(
                        "INSERT INTO research_questions (id, project_id, parent_id, code, question, objective, category, display_order, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                        params![id, input.project_id, input.parent_id, code, question, input.objective.trim(), input.category.trim(), order, timestamp],
                    )
                    .map_err(|_| AppError::Validation(format!("Ya existe una pregunta con el código {code}")))?;
                audit(
                    &connection,
                    &input.project_id,
                    "research_question",
                    &id,
                    "created",
                    &actor,
                    None,
                    json!({ "code": code, "question": question }),
                )?;
            }
        }
        self.research_questions(&input.project_id)
    }

    pub fn delete_research_question(
        &self,
        project_id: &str,
        question_id: &str,
    ) -> AppResult<QuestionsOverview> {
        let connection = self.open()?;
        let actor = active_reviewer(&connection, project_id)?;
        let values: i64 = connection.query_row(
            "SELECT COUNT(*) FROM extracted_values v JOIN extraction_fields f ON f.id = v.field_id WHERE f.research_question_id = ?1",
            [question_id],
            |row| row.get(0),
        )?;
        require(
            values == 0,
            "La pregunta tiene valores extraídos; elimina o mueve primero sus aspectos",
        )?;
        connection.execute(
            "UPDATE extraction_fields SET deleted_at = ?1 WHERE research_question_id = ?2",
            params![now(), question_id],
        )?;
        let changed = connection.execute(
            "DELETE FROM research_questions WHERE id = ?1 AND project_id = ?2",
            params![question_id, project_id],
        )?;
        require(changed > 0, "La pregunta no existe")?;
        audit(
            &connection,
            project_id,
            "research_question",
            question_id,
            "deleted",
            &actor,
            None,
            json!({}),
        )?;
        self.research_questions(project_id)
    }

    pub fn save_extraction_field(&self, input: FieldInput) -> AppResult<QuestionsOverview> {
        let name = input.name.trim();
        require(
            (1..=120).contains(&name.chars().count()),
            "El aspecto necesita un nombre de hasta 120 caracteres",
        )?;
        require(
            FIELD_TYPES.contains(&input.data_type.as_str()),
            "Tipo de dato no reconocido",
        )?;
        require(
            input.data_type != "category" || !input.taxonomy.is_empty(),
            "Un aspecto categórico necesita al menos una categoría",
        )?;
        let connection = self.open()?;
        ensure_project(&connection, &input.project_id)?;
        if let Some(question) = &input.research_question_id {
            let belongs: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM research_questions WHERE id = ?1 AND project_id = ?2)",
                params![question, input.project_id],
                |row| row.get(0),
            )?;
            require(belongs, "La pregunta no pertenece a esta revisión")?;
        }
        let actor = active_reviewer(&connection, &input.project_id)?;
        let clean = |items: &[String]| -> Vec<String> {
            items
                .iter()
                .map(|item| item.trim().to_owned())
                .filter(|item| !item.is_empty())
                .collect()
        };
        let taxonomy = to_json(&clean(&input.taxonomy))?;
        let examples = to_json(&clean(&input.examples))?;
        let master = input.master_field_key.filter(|key| !key.is_empty());
        match &input.id {
            Some(id) => {
                let changed = connection.execute(
                    "UPDATE extraction_fields SET research_question_id = ?1, name = ?2, description = ?3, data_type = ?4, unit = ?5, taxonomy_json = ?6,
                            instructions = ?7, required = ?8, examples_json = ?9, master_field_key = ?10
                     WHERE id = ?11 AND project_id = ?12 AND deleted_at IS NULL",
                    params![input.research_question_id, name, input.description.trim(), input.data_type, input.unit, taxonomy, input.instructions.trim(), input.required, examples, master, id, input.project_id],
                )?;
                require(changed > 0, "El aspecto no existe")?;
                audit(
                    &connection,
                    &input.project_id,
                    "extraction_field",
                    id,
                    "updated",
                    &actor,
                    None,
                    json!({ "name": name, "type": input.data_type }),
                )?;
            }
            None => {
                let id = Uuid::new_v4().to_string();
                let order: i64 = connection.query_row("SELECT COALESCE(MAX(display_order), 0) + 1 FROM extraction_fields WHERE project_id = ?1", [&input.project_id], |row| row.get(0))?;
                connection.execute(
                    "INSERT INTO extraction_fields (id, project_id, research_question_id, name, description, data_type, unit, taxonomy_json, instructions, required, examples_json, master_field_key, display_order)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![id, input.project_id, input.research_question_id, name, input.description.trim(), input.data_type, input.unit, taxonomy, input.instructions.trim(), input.required, examples, master, order],
                )?;
                audit(
                    &connection,
                    &input.project_id,
                    "extraction_field",
                    &id,
                    "created",
                    &actor,
                    None,
                    json!({ "name": name, "type": input.data_type }),
                )?;
            }
        }
        self.research_questions(&input.project_id)
    }

    pub fn delete_extraction_field(
        &self,
        project_id: &str,
        field_id: &str,
    ) -> AppResult<QuestionsOverview> {
        let connection = self.open()?;
        let actor = active_reviewer(&connection, project_id)?;
        let changed = connection.execute("UPDATE extraction_fields SET deleted_at = ?1 WHERE id = ?2 AND project_id = ?3 AND deleted_at IS NULL", params![now(), field_id, project_id])?;
        require(changed > 0, "El aspecto no existe")?;
        audit(
            &connection,
            project_id,
            "extraction_field",
            field_id,
            "deleted",
            &actor,
            None,
            json!({ "note": "Los valores extraídos se conservan y el aspecto puede restaurarse desde la auditoría" }),
        )?;
        self.research_questions(project_id)
    }
}

// --- Search strategies (Cadena) -----------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchImportInfo {
    pub source_path: String,
    pub source_sha256: String,
    pub imported_count: i64,
    pub duplicate_count: i64,
    pub linked_count: i64,
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStrategy {
    pub id: String,
    pub database_name: String,
    pub query_text: String,
    pub query_date: String,
    pub result_count: i64,
    pub notes: String,
    pub version_number: i64,
    pub filters: Value,
    pub language: Option<String>,
    pub period_start: Option<i64>,
    pub period_end: Option<i64>,
    pub records_imported: i64,
    pub imports: Vec<SearchImportInfo>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyInput {
    pub id: Option<String>,
    pub project_id: String,
    pub database_name: String,
    pub query_text: String,
    pub query_date: String,
    pub result_count: i64,
    pub notes: String,
    pub filters: Value,
    pub language: Option<String>,
    pub period_start: Option<i64>,
    pub period_end: Option<i64>,
    /// Save as a new version of `id` instead of editing it.
    #[serde(default)]
    pub new_version: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyImportResult {
    pub records: i64,
    pub new_articles: i64,
    pub duplicates: i64,
    pub errors: i64,
    pub added_to_corpus: i64,
    pub file_already_imported: bool,
}

impl Database {
    pub fn search_strategies(&self, project_id: &str) -> AppResult<Vec<SearchStrategy>> {
        let connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let mut statement = connection.prepare(
            "SELECT id, database_name, query_text, query_date, result_count, notes, version_number, filters_json, language, period_start, period_end,
                    (SELECT COUNT(*) FROM search_imports si JOIN source_records sr ON sr.import_batch_id = si.import_batch_id
                     WHERE si.search_strategy_id = s.id AND sr.record_status <> 'error')
             FROM search_strategies s WHERE project_id = ?1 AND deleted_at IS NULL ORDER BY database_name, version_number DESC",
        )?;
        let mut strategies = statement
            .query_map([project_id], |row| {
                Ok(SearchStrategy {
                    id: row.get(0)?,
                    database_name: row.get(1)?,
                    query_text: row.get(2)?,
                    query_date: row.get(3)?,
                    result_count: row.get(4)?,
                    notes: row.get(5)?,
                    version_number: row.get(6)?,
                    filters: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or(json!({})),
                    language: row.get(8)?,
                    period_start: row.get(9)?,
                    period_end: row.get(10)?,
                    records_imported: row.get(11)?,
                    imports: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut imports = connection.prepare(
            "SELECT source_path, source_sha256, imported_count, duplicate_count, linked_count, imported_at FROM search_imports WHERE search_strategy_id = ?1 ORDER BY imported_at",
        )?;
        for strategy in &mut strategies {
            strategy.imports = imports
                .query_map([&strategy.id], |row| {
                    Ok(SearchImportInfo {
                        source_path: row.get(0)?,
                        source_sha256: row.get(1)?,
                        imported_count: row.get(2)?,
                        duplicate_count: row.get(3)?,
                        linked_count: row.get(4)?,
                        imported_at: row.get(5)?,
                    })
                })?
                .collect::<Result<_, _>>()?;
        }
        Ok(strategies)
    }

    pub fn save_search_strategy(&self, input: StrategyInput) -> AppResult<Vec<SearchStrategy>> {
        let database_name = input.database_name.trim();
        require(
            (2..=80).contains(&database_name.chars().count()),
            "Indica la base de datos (2 a 80 caracteres)",
        )?;
        require(
            !input.query_text.trim().is_empty(),
            "La cadena de búsqueda no puede estar vacía",
        )?;
        require(
            chrono::NaiveDate::parse_from_str(input.query_date.trim(), "%Y-%m-%d").is_ok(),
            "La fecha debe tener formato AAAA-MM-DD",
        )?;
        require(
            input.result_count >= 0,
            "La cantidad de resultados no puede ser negativa",
        )?;
        require(input.filters.is_object(), "Los filtros deben ser un objeto")?;
        let connection = self.open()?;
        ensure_project(&connection, &input.project_id)?;
        let actor = active_reviewer(&connection, &input.project_id)?;
        match (&input.id, input.new_version) {
            (Some(id), false) => {
                let changed = connection.execute(
                    "UPDATE search_strategies SET database_name = ?1, query_text = ?2, query_date = ?3, result_count = ?4, notes = ?5, filters_json = ?6,
                            language = ?7, period_start = ?8, period_end = ?9 WHERE id = ?10 AND project_id = ?11 AND deleted_at IS NULL",
                    params![database_name, input.query_text.trim(), input.query_date.trim(), input.result_count, input.notes.trim(), input.filters.to_string(), input.language, input.period_start, input.period_end, id, input.project_id],
                )?;
                require(changed > 0, "La estrategia no existe")?;
                audit(
                    &connection,
                    &input.project_id,
                    "search_strategy",
                    id,
                    "updated",
                    &actor,
                    None,
                    json!({ "database": database_name, "query": input.query_text.trim() }),
                )?;
            }
            (previous, _) => {
                let version: i64 = match previous {
                    Some(id) => connection.query_row(
                        "SELECT version_number + 1 FROM search_strategies WHERE id = ?1",
                        [id],
                        |row| row.get(0),
                    )?,
                    None => 1,
                };
                let id = Uuid::new_v4().to_string();
                connection.execute(
                    "INSERT INTO search_strategies (id, project_id, database_name, query_text, query_date, result_count, notes, created_at, version_number, filters_json, language, period_start, period_end)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![id, input.project_id, database_name, input.query_text.trim(), input.query_date.trim(), input.result_count, input.notes.trim(), now(), version, input.filters.to_string(), input.language, input.period_start, input.period_end],
                )?;
                audit(
                    &connection,
                    &input.project_id,
                    "search_strategy",
                    &id,
                    "created",
                    &actor,
                    None,
                    json!({ "database": database_name, "version": version }),
                )?;
            }
        }
        self.search_strategies(&input.project_id)
    }

    pub fn delete_search_strategy(
        &self,
        project_id: &str,
        strategy_id: &str,
    ) -> AppResult<Vec<SearchStrategy>> {
        let connection = self.open()?;
        let actor = active_reviewer(&connection, project_id)?;
        let changed = connection.execute("UPDATE search_strategies SET deleted_at = ?1 WHERE id = ?2 AND project_id = ?3 AND deleted_at IS NULL", params![now(), strategy_id, project_id])?;
        require(changed > 0, "La estrategia no existe")?;
        audit(
            &connection,
            project_id,
            "search_strategy",
            strategy_id,
            "archived",
            &actor,
            None,
            json!({}),
        )?;
        self.search_strategies(project_id)
    }

    /// Import an exported results file (RIS/BibTeX/CSV) for a strategy and add its
    /// records to the project corpus. Records keep their per-database provenance.
    pub fn import_strategy_results(
        &self,
        strategy_id: &str,
        path: &str,
    ) -> AppResult<StrategyImportResult> {
        let (project_id, database_name): (String, String) = self
            .open()?
            .query_row("SELECT project_id, database_name FROM search_strategies WHERE id = ?1 AND deleted_at IS NULL", [strategy_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional()?
            .ok_or_else(|| AppError::Validation("La estrategia no existe".into()))?;
        let (batch_id, results) = self.import_reference_file(path)?;
        let batch_id = batch_id.ok_or_else(|| {
            AppError::Validation(
                results
                    .first()
                    .and_then(|result| result.message.clone())
                    .unwrap_or_else(|| "No se pudo importar el archivo".into()),
            )
        })?;
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        let (records, new_articles, duplicates, errors, sha): (i64, i64, i64, i64, String) = transaction.query_row(
            "SELECT COUNT(sr.id), COALESCE(SUM(sr.record_status = 'imported'), 0), COALESCE(SUM(sr.record_status = 'duplicate'), 0),
                    COALESCE(SUM(sr.record_status = 'error'), 0), b.source_sha256
             FROM import_batches b LEFT JOIN source_records sr ON sr.import_batch_id = b.id WHERE b.id = ?1 GROUP BY b.id",
            [&batch_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )?;
        let already = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM search_imports WHERE search_strategy_id = ?1 AND import_batch_id = ?2)",
            params![strategy_id, batch_id],
            |row| row.get::<_, bool>(0),
        )?;
        let article_ids: Vec<String> = {
            let mut statement = transaction.prepare("SELECT DISTINCT article_id FROM source_records WHERE import_batch_id = ?1 AND article_id IS NOT NULL")?;
            statement
                .query_map([&batch_id], |row| row.get(0))?
                .collect::<Result<_, _>>()?
        };
        let added = add_articles(
            &transaction,
            &project_id,
            &article_ids,
            Some(&database_name),
        )?;
        if !already {
            transaction.execute(
                "INSERT INTO search_imports (id, search_strategy_id, source_path, source_sha256, imported_count, duplicate_count, imported_at, import_batch_id, linked_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![Uuid::new_v4().to_string(), strategy_id, path, sha, new_articles, duplicates, now(), batch_id, added],
            )?;
        }
        let actor = active_reviewer(&transaction, &project_id)?;
        audit(
            &transaction,
            &project_id,
            "search_strategy",
            strategy_id,
            "results_imported",
            &actor,
            None,
            json!({ "records": records, "addedToCorpus": added, "sha256": sha }),
        )?;
        transaction.commit()?;
        Ok(StrategyImportResult {
            records,
            new_articles,
            duplicates,
            errors,
            added_to_corpus: added,
            file_already_imported: already,
        })
    }

    pub fn project_audit(&self, project_id: &str, limit: i64) -> AppResult<Vec<AuditEntry>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT entity_type, entity_id, action, actor_type, actor_name, before_json, after_json, created_at
             FROM audit_events WHERE project_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT ?2",
        )?;
        Ok(statement
            .query_map(params![project_id, limit.clamp(1, 2000)], |row| {
                Ok(AuditEntry {
                    entity_type: row.get(0)?,
                    entity_id: row.get(1)?,
                    action: row.get(2)?,
                    actor_type: row.get(3)?,
                    actor_name: row.get(4)?,
                    before: row
                        .get::<_, Option<String>>(5)?
                        .and_then(|text| serde_json::from_str(&text).ok()),
                    after: row
                        .get::<_, Option<String>>(6)?
                        .and_then(|text| serde_json::from_str(&text).ok()),
                    created_at: row.get(7)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub action: String,
    pub actor_type: String,
    pub actor_name: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub created_at: String,
}

#[cfg(test)]
#[path = "review_tests.rs"]
mod tests;
