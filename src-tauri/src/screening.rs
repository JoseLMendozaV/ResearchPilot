//! Screening (sheet I_E): configurable criteria, per-reviewer decisions with
//! criterion marks, final decisions (direct in single mode, by agreement or
//! resolution in double mode), conflicts, derived workflow stages and
//! inter-rater agreement. AI may recommend in later phases; only people decide.

use std::collections::{BTreeMap, HashMap};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    db::{Database, now},
    error::{AppError, AppResult},
    review::{Actor, active_reviewer, audit, ensure_project},
};

pub const DECISIONS: [&str; 4] = ["include", "exclude", "maybe", "awaiting_full_text"];

fn require(condition: bool, message: &str) -> AppResult<()> {
    if condition {
        Ok(())
    } else {
        Err(AppError::Validation(message.into()))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Criterion {
    pub id: String,
    pub criterion_type: String,
    pub code: String,
    pub name: String,
    pub description: String,
    pub active: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CriterionInput {
    pub id: Option<String>,
    pub project_id: String,
    pub criterion_type: String,
    pub code: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionInfo {
    pub id: String,
    pub reviewer_id: Option<String>,
    pub reviewer_name: String,
    pub decision: String,
    pub criterion_id: Option<String>,
    pub criteria: Value,
    pub reason: String,
    pub is_final: bool,
    pub decided_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreeningRow {
    pub article_id: String,
    pub sequence: i64,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub venue: Option<String>,
    pub doi: Option<String>,
    pub keywords: Vec<String>,
    pub abstract_text: Option<String>,
    pub stage: String,
    pub is_secondary: bool,
    pub source_database: Option<String>,
    pub has_pdf: bool,
    /// Latest decision of each reviewer in this phase.
    pub reviews: Vec<DecisionInfo>,
    pub final_decision: Option<DecisionInfo>,
    pub conflict_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agreement {
    pub reviewer_a: String,
    pub reviewer_b: String,
    pub compared: i64,
    pub observed: f64,
    pub kappa: Option<f64>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreeningTable {
    pub phase: String,
    pub criteria: Vec<Criterion>,
    pub rows: Vec<ScreeningRow>,
    pub agreement: Option<Agreement>,
    pub decided: i64,
    pub pending: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionInput {
    pub project_id: String,
    pub article_id: String,
    pub phase: String,
    pub decision: String,
    pub criterion_id: Option<String>,
    #[serde(default)]
    pub criteria: BTreeMap<String, String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub id: String,
    pub article_id: String,
    pub sequence: Option<i64>,
    pub title: String,
    pub conflict_type: String,
    pub phase: Option<String>,
    pub status: String,
    pub details: Value,
    pub resolution: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

impl Database {
    pub fn eligibility_criteria(&self, project_id: &str) -> AppResult<Vec<Criterion>> {
        criteria(&self.open()?, project_id)
    }

    pub fn save_criterion(&self, input: CriterionInput) -> AppResult<Vec<Criterion>> {
        require(
            ["inclusion", "exclusion"].contains(&input.criterion_type.as_str()),
            "El criterio debe ser de inclusión o de exclusión",
        )?;
        let code = input.code.trim();
        let name = input.name.trim();
        require(
            (1..=20).contains(&code.chars().count()),
            "El código del criterio debe tener entre 1 y 20 caracteres",
        )?;
        require(
            (2..=200).contains(&name.chars().count()),
            "El criterio necesita un nombre",
        )?;
        let connection = self.open()?;
        ensure_project(&connection, &input.project_id)?;
        let actor = active_reviewer(&connection, &input.project_id)?;
        let result = match &input.id {
            Some(id) => connection.execute(
                "UPDATE eligibility_criteria SET criterion_type = ?1, code = ?2, name = ?3, description = ?4, active = 1 WHERE id = ?5 AND project_id = ?6",
                params![input.criterion_type, code, name, input.description.trim(), id, input.project_id],
            ),
            None => {
                let order: i64 = connection.query_row("SELECT COALESCE(MAX(display_order), 0) + 1 FROM eligibility_criteria WHERE project_id = ?1", [&input.project_id], |row| row.get(0))?;
                connection.execute(
                    "INSERT INTO eligibility_criteria (id, project_id, criterion_type, code, name, description, display_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![Uuid::new_v4().to_string(), input.project_id, input.criterion_type, code, name, input.description.trim(), order],
                )
            }
        };
        result.map_err(|_| {
            AppError::Validation(format!("Ya existe un criterio con el código {code}"))
        })?;
        audit(
            &connection,
            &input.project_id,
            "eligibility_criterion",
            input.id.as_deref().unwrap_or(code),
            if input.id.is_some() {
                "updated"
            } else {
                "created"
            },
            &actor,
            None,
            json!({ "code": code, "name": name, "type": input.criterion_type }),
        )?;
        criteria(&connection, &input.project_id)
    }

    /// Criteria already used in decisions are deactivated, never deleted.
    pub fn delete_criterion(
        &self,
        project_id: &str,
        criterion_id: &str,
    ) -> AppResult<Vec<Criterion>> {
        let connection = self.open()?;
        let actor = active_reviewer(&connection, project_id)?;
        let used: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM screening_decisions WHERE criterion_id = ?1 OR instr(criteria_json, ?1) > 0)",
            [criterion_id],
            |row| row.get(0),
        )?;
        if used {
            connection.execute(
                "UPDATE eligibility_criteria SET active = 0 WHERE id = ?1 AND project_id = ?2",
                params![criterion_id, project_id],
            )?;
        } else {
            connection.execute(
                "DELETE FROM eligibility_criteria WHERE id = ?1 AND project_id = ?2",
                params![criterion_id, project_id],
            )?;
        }
        audit(
            &connection,
            project_id,
            "eligibility_criterion",
            criterion_id,
            if used { "deactivated" } else { "deleted" },
            &actor,
            None,
            json!({}),
        )?;
        criteria(&connection, project_id)
    }

    pub fn screening_table(&self, project_id: &str, phase: &str) -> AppResult<ScreeningTable> {
        require(
            ["metadata", "full_text"].contains(&phase),
            "Fase de cribado no reconocida",
        )?;
        let connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let rows = screening_rows(&connection, project_id, phase, None)?;
        let decided = rows
            .iter()
            .filter(|row| row.final_decision.is_some())
            .count() as i64;
        Ok(ScreeningTable {
            phase: phase.into(),
            criteria: criteria(&connection, project_id)?,
            pending: rows.len() as i64 - decided,
            decided,
            agreement: agreement(&rows),
            rows,
        })
    }

    pub fn record_screening_decision(&self, input: DecisionInput) -> AppResult<ScreeningRow> {
        require(
            ["metadata", "full_text"].contains(&input.phase.as_str()),
            "Fase de cribado no reconocida",
        )?;
        require(
            DECISIONS.contains(&input.decision.as_str()),
            "Decisión no reconocida",
        )?;
        require(
            input
                .criteria
                .values()
                .all(|mark| ["met", "not_met", "unclear"].contains(&mark.as_str())),
            "Marca de criterio no reconocida",
        )?;
        require(
            input.decision != "exclude"
                || input.criterion_id.is_some()
                || input.reason.trim().len() >= 3,
            "Una exclusión necesita el criterio aplicado o una justificación",
        )?;
        require(
            input.phase != "full_text"
                || input.decision != "exclude"
                || input.criterion_id.is_some(),
            "En texto completo, toda exclusión debe indicar el criterio (motivo de exclusión)",
        )?;
        let mut connection = self.open()?;
        ensure_project(&connection, &input.project_id)?;
        let transaction = connection.transaction()?;
        let member: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM review_articles WHERE project_id = ?1 AND article_id = ?2 AND removed_at IS NULL)",
            params![input.project_id, input.article_id],
            |row| row.get(0),
        )?;
        require(member, "El artículo no forma parte de esta revisión")?;
        if let Some(criterion) = &input.criterion_id {
            let valid: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM eligibility_criteria WHERE id = ?1 AND project_id = ?2)", params![criterion, input.project_id], |row| row.get(0))?;
            require(valid, "El criterio no pertenece a esta revisión")?;
        }
        let actor = active_reviewer(&transaction, &input.project_id)?;
        let mode: String = transaction.query_row(
            "SELECT screening_mode FROM review_projects WHERE id = ?1",
            [&input.project_id],
            |row| row.get(0),
        )?;
        let criteria_json = serde_json::to_string(&input.criteria)
            .map_err(|error| AppError::Validation(error.to_string()))?;
        insert_decision(
            &transaction,
            &input,
            &actor,
            &criteria_json,
            mode == "single",
        )?;
        if mode == "double" {
            reconcile(
                &transaction,
                &input.project_id,
                &input.article_id,
                &input.phase,
            )?;
        }
        refresh_stage(&transaction, &input.project_id, &input.article_id)?;
        audit(
            &transaction,
            &input.project_id,
            "screening_decision",
            &input.article_id,
            "decided",
            &actor,
            None,
            json!({ "phase": input.phase, "decision": input.decision, "criterion": input.criterion_id, "reason": input.reason.trim() }),
        )?;
        transaction.commit()?;
        self.screening_row(&input.project_id, &input.article_id, &input.phase)
    }

    /// Final decision by a resolver for an open screening conflict.
    pub fn resolve_screening_conflict(
        &self,
        conflict_id: &str,
        decision: &str,
        criterion_id: Option<String>,
        reason: &str,
    ) -> AppResult<ScreeningRow> {
        require(DECISIONS.contains(&decision), "Decisión no reconocida")?;
        require(
            reason.trim().len() >= 3,
            "Explica la resolución del conflicto",
        )?;
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        let (project_id, article_id, phase): (String, String, String) = transaction
            .query_row(
                "SELECT project_id, article_id, COALESCE(phase, 'metadata') FROM reviewer_conflicts WHERE id = ?1 AND status = 'open' AND conflict_type = 'screening'",
                [conflict_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::Validation("El conflicto no existe o ya fue resuelto".into()))?;
        let actor = active_reviewer(&transaction, &project_id)?;
        let input = DecisionInput {
            project_id: project_id.clone(),
            article_id: article_id.clone(),
            phase: phase.clone(),
            decision: decision.into(),
            criterion_id,
            criteria: BTreeMap::new(),
            reason: format!("Resolución de conflicto: {}", reason.trim()),
        };
        insert_decision(&transaction, &input, &actor, "{}", true)?;
        transaction.execute(
            "UPDATE reviewer_conflicts SET status = 'resolved', resolution = ?1, resolver_id = ?2, resolved_at = ?3 WHERE id = ?4",
            params![format!("{decision}: {}", reason.trim()), actor.reviewer_id, now(), conflict_id],
        )?;
        refresh_stage(&transaction, &project_id, &article_id)?;
        audit(
            &transaction,
            &project_id,
            "reviewer_conflict",
            conflict_id,
            "resolved",
            &actor,
            None,
            json!({ "decision": decision, "reason": reason.trim() }),
        )?;
        transaction.commit()?;
        self.screening_row(&project_id, &article_id, &phase)
    }

    pub fn conflicts(&self, project_id: &str) -> AppResult<Vec<ConflictInfo>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT c.id, c.article_id, r.sequence, a.title, c.conflict_type, c.phase, c.status, c.details_json, c.resolution, c.created_at, c.resolved_at
             FROM reviewer_conflicts c JOIN articles a ON a.id = c.article_id
             LEFT JOIN review_articles r ON r.project_id = c.project_id AND r.article_id = c.article_id
             WHERE c.project_id = ?1 ORDER BY c.status DESC, c.created_at DESC",
        )?;
        Ok(statement
            .query_map([project_id], |row| {
                Ok(ConflictInfo {
                    id: row.get(0)?,
                    article_id: row.get(1)?,
                    sequence: row.get(2)?,
                    title: row.get(3)?,
                    conflict_type: row.get(4)?,
                    phase: row.get(5)?,
                    status: row.get(6)?,
                    details: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or(Value::Null),
                    resolution: row.get(8)?,
                    created_at: row.get(9)?,
                    resolved_at: row.get(10)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }

    fn screening_row(
        &self,
        project_id: &str,
        article_id: &str,
        phase: &str,
    ) -> AppResult<ScreeningRow> {
        require(
            ["metadata", "full_text"].contains(&phase),
            "Fase de cribado no reconocida",
        )?;
        screening_rows(&self.open()?, project_id, phase, Some(article_id))?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Validation("El artículo ya no aparece en esta fase".into()))
    }
}

/// Rows of a screening phase; `article_id` limits it to one study.
fn screening_rows(
    connection: &Connection,
    project_id: &str,
    phase: &str,
    article_id: Option<&str>,
) -> AppResult<Vec<ScreeningRow>> {
    // Full text only considers articles that passed the metadata screening.
    let stage_filter = if phase == "full_text" {
        "AND (r.workflow_stage IN ('eligible', 'included') OR EXISTS(SELECT 1 FROM screening_decisions d WHERE d.project_id = r.project_id AND d.article_id = r.article_id AND d.phase = 'full_text'))"
    } else {
        ""
    };
    let mut statement = connection.prepare(&format!(
            "SELECT a.id, r.sequence, a.title, a.authors_json, a.publication_year, a.venue, a.doi, a.keywords_json, a.abstract_text,
                    r.workflow_stage, r.is_secondary, r.source_database, a.pdf_path IS NOT NULL,
                    (SELECT id FROM reviewer_conflicts c WHERE c.project_id = r.project_id AND c.article_id = a.id AND c.conflict_type = 'screening' AND c.phase = ?2 AND c.status = 'open' LIMIT 1)
             FROM review_articles r JOIN articles a ON a.id = r.article_id
             WHERE r.project_id = ?1 AND (?3 IS NULL OR r.article_id = ?3) AND r.removed_at IS NULL AND a.deleted_at IS NULL {stage_filter}
             ORDER BY r.sequence"
        ))?;
    let mut rows: Vec<ScreeningRow> = statement
        .query_map(params![project_id, phase, article_id], |row| {
            Ok(ScreeningRow {
                article_id: row.get(0)?,
                sequence: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                title: row.get(2)?,
                authors: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                year: row.get(4)?,
                venue: row.get(5)?,
                doi: row.get(6)?,
                keywords: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                abstract_text: row.get(8)?,
                stage: row.get(9)?,
                is_secondary: row.get(10)?,
                source_database: row.get(11)?,
                has_pdf: row.get(12)?,
                reviews: Vec::new(),
                final_decision: None,
                conflict_id: row.get(13)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    for row in &mut rows {
        row.reviews = latest_reviews(connection, project_id, &row.article_id, phase)?;
        row.final_decision = final_decision(connection, project_id, &row.article_id, phase)?;
    }
    Ok(rows)
}

fn criteria(connection: &Connection, project_id: &str) -> AppResult<Vec<Criterion>> {
    let mut statement = connection.prepare(
        "SELECT id, criterion_type, code, name, description, active FROM eligibility_criteria WHERE project_id = ?1
         ORDER BY CASE criterion_type WHEN 'inclusion' THEN 0 ELSE 1 END, display_order",
    )?;
    Ok(statement
        .query_map([project_id], |row| {
            Ok(Criterion {
                id: row.get(0)?,
                criterion_type: row.get(1)?,
                code: row.get(2)?,
                name: row.get(3)?,
                description: row.get(4)?,
                active: row.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

fn insert_decision(
    connection: &Connection,
    input: &DecisionInput,
    actor: &Actor,
    criteria_json: &str,
    is_final: bool,
) -> AppResult<()> {
    if is_final {
        connection.execute(
            "UPDATE screening_decisions SET is_final = 0 WHERE project_id = ?1 AND article_id = ?2 AND phase = ?3",
            params![input.project_id, input.article_id, input.phase],
        )?;
    }
    connection.execute(
        "INSERT INTO screening_decisions (id, project_id, article_id, phase, decision, criterion_id, reviewer_id, actor_type, actor_name, reason, is_final, decided_at, criteria_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'human', ?8, ?9, ?10, ?11, ?12)",
        params![Uuid::new_v4().to_string(), input.project_id, input.article_id, input.phase, input.decision, input.criterion_id, actor.reviewer_id, actor.name, input.reason.trim(), is_final, now(), criteria_json],
    )?;
    Ok(())
}

const DECISION_SELECT: &str = "SELECT id, reviewer_id, actor_name, decision, criterion_id, criteria_json, reason, is_final, decided_at FROM screening_decisions";

fn map_decision(row: &rusqlite::Row<'_>) -> rusqlite::Result<DecisionInfo> {
    Ok(DecisionInfo {
        id: row.get(0)?,
        reviewer_id: row.get(1)?,
        reviewer_name: row.get(2)?,
        decision: row.get(3)?,
        criterion_id: row.get(4)?,
        criteria: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or(json!({})),
        reason: row.get(6)?,
        is_final: row.get(7)?,
        decided_at: row.get(8)?,
    })
}

/// Latest non-system decision of each reviewer.
fn latest_reviews(
    connection: &Connection,
    project_id: &str,
    article_id: &str,
    phase: &str,
) -> AppResult<Vec<DecisionInfo>> {
    let mut statement = connection.prepare(&format!(
        "{DECISION_SELECT} d WHERE project_id = ?1 AND article_id = ?2 AND phase = ?3 AND reviewer_id IS NOT NULL AND reason NOT LIKE 'Resolución de conflicto:%'
           AND decided_at = (SELECT MAX(decided_at) FROM screening_decisions o WHERE o.project_id = d.project_id AND o.article_id = d.article_id
                             AND o.phase = d.phase AND o.reviewer_id IS d.reviewer_id AND o.reason NOT LIKE 'Resolución de conflicto:%')
         ORDER BY actor_name"
    ))?;
    Ok(statement
        .query_map(params![project_id, article_id, phase], map_decision)?
        .collect::<Result<_, _>>()?)
}

fn final_decision(
    connection: &Connection,
    project_id: &str,
    article_id: &str,
    phase: &str,
) -> AppResult<Option<DecisionInfo>> {
    Ok(connection
        .query_row(
            &format!("{DECISION_SELECT} WHERE project_id = ?1 AND article_id = ?2 AND phase = ?3 AND is_final = 1 ORDER BY decided_at DESC, rowid DESC LIMIT 1"),
            params![project_id, article_id, phase],
            map_decision,
        )
        .optional()?)
}

/// Double review: agreement finalizes; disagreement opens a conflict.
pub(crate) fn reconcile(
    connection: &Connection,
    project_id: &str,
    article_id: &str,
    phase: &str,
) -> AppResult<()> {
    let reviews = latest_reviews(connection, project_id, article_id, phase)?;
    let open: Option<String> = connection
        .query_row(
            "SELECT id FROM reviewer_conflicts WHERE project_id = ?1 AND article_id = ?2 AND conflict_type = 'screening' AND phase = ?3 AND status = 'open'",
            params![project_id, article_id, phase],
            |row| row.get(0),
        )
        .optional()?;
    let resolved_by_resolver: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM screening_decisions WHERE project_id = ?1 AND article_id = ?2 AND phase = ?3 AND is_final = 1 AND reason LIKE 'Resolución de conflicto:%')",
        params![project_id, article_id, phase],
        |row| row.get(0),
    )?;
    if reviews.len() < 2 || resolved_by_resolver {
        return Ok(());
    }
    let first = &reviews[0].decision;
    if reviews.iter().all(|review| &review.decision == first) {
        connection.execute("UPDATE screening_decisions SET is_final = 0 WHERE project_id = ?1 AND article_id = ?2 AND phase = ?3", params![project_id, article_id, phase])?;
        connection.execute(
            "INSERT INTO screening_decisions (id, project_id, article_id, phase, decision, criterion_id, actor_type, actor_name, reason, is_final, decided_at, criteria_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'human', 'Acuerdo entre revisores', ?7, 1, ?8, '{}')",
            params![Uuid::new_v4().to_string(), project_id, article_id, phase, first, reviews[0].criterion_id, format!("Acuerdo de {} revisores", reviews.len()), now()],
        )?;
        if let Some(conflict) = open {
            connection.execute(
                "UPDATE reviewer_conflicts SET status = 'resolved', resolution = 'Los revisores coincidieron tras revisar', resolved_at = ?1 WHERE id = ?2",
                params![now(), conflict],
            )?;
        }
    } else {
        connection.execute("UPDATE screening_decisions SET is_final = 0 WHERE project_id = ?1 AND article_id = ?2 AND phase = ?3", params![project_id, article_id, phase])?;
        let details = json!({ "decisions": reviews.iter().map(|review| json!({ "reviewer": review.reviewer_name, "decision": review.decision, "reason": review.reason })).collect::<Vec<_>>() });
        match open {
            Some(conflict) => {
                connection.execute(
                    "UPDATE reviewer_conflicts SET details_json = ?1 WHERE id = ?2",
                    params![details.to_string(), conflict],
                )?;
            }
            None => {
                connection.execute(
                    "INSERT INTO reviewer_conflicts (id, project_id, article_id, conflict_type, status, created_at, phase, details_json)
                     VALUES (?1, ?2, ?3, 'screening', 'open', ?4, ?5, ?6)",
                    params![Uuid::new_v4().to_string(), project_id, article_id, now(), phase, details.to_string()],
                )?;
            }
        }
    }
    Ok(())
}

/// Stage derived only from final decisions, so it always matches the record.
pub(crate) fn refresh_stage(
    connection: &Connection,
    project_id: &str,
    article_id: &str,
) -> AppResult<()> {
    let full_text = final_decision(connection, project_id, article_id, "full_text")?
        .map(|decision| decision.decision);
    let metadata = final_decision(connection, project_id, article_id, "metadata")?
        .map(|decision| decision.decision);
    let any_decision: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM screening_decisions WHERE project_id = ?1 AND article_id = ?2)",
        params![project_id, article_id],
        |row| row.get(0),
    )?;
    let stage = match (full_text.as_deref(), metadata.as_deref()) {
        (Some("include"), _) => "included",
        (Some("exclude"), _) => "excluded",
        (Some(_), _) => "eligible",
        (None, Some("include" | "awaiting_full_text")) => "eligible",
        (None, Some("exclude")) => "excluded",
        (None, Some(_)) => "screening",
        (None, None) if any_decision => "screening",
        (None, None) => "identified",
    };
    connection.execute(
        "UPDATE review_articles SET workflow_stage = ?1 WHERE project_id = ?2 AND article_id = ?3",
        params![stage, project_id, article_id],
    )?;
    Ok(())
}

/// Cohen's kappa between the two reviewers with most decisions in common.
fn agreement(rows: &[ScreeningRow]) -> Option<Agreement> {
    let mut by_pair: HashMap<(String, String), Vec<(String, String)>> = HashMap::new();
    for row in rows {
        for (index, first) in row.reviews.iter().enumerate() {
            for second in &row.reviews[index + 1..] {
                let (a, b) = if first.reviewer_name <= second.reviewer_name {
                    (first, second)
                } else {
                    (second, first)
                };
                by_pair
                    .entry((a.reviewer_name.clone(), b.reviewer_name.clone()))
                    .or_default()
                    .push((a.decision.clone(), b.decision.clone()));
            }
        }
    }
    let ((reviewer_a, reviewer_b), pairs) =
        by_pair.into_iter().max_by_key(|(_, pairs)| pairs.len())?;
    let n = pairs.len() as f64;
    let observed = pairs.iter().filter(|(a, b)| a == b).count() as f64 / n;
    let expected: f64 = DECISIONS
        .iter()
        .map(|category| {
            let a = pairs.iter().filter(|(left, _)| left == category).count() as f64 / n;
            let b = pairs.iter().filter(|(_, right)| right == category).count() as f64 / n;
            a * b
        })
        .sum();
    let kappa =
        ((1.0 - expected).abs() > f64::EPSILON).then(|| (observed - expected) / (1.0 - expected));
    let note = if pairs.len() < 20 {
        "Muestra pequeña (menos de 20 artículos en común): interpreta el kappa con cautela."
            .to_owned()
    } else if kappa.is_none() {
        "Kappa no definido: ambos revisores usaron una sola categoría.".to_owned()
    } else {
        String::new()
    };
    Some(Agreement {
        reviewer_a,
        reviewer_b,
        compared: pairs.len() as i64,
        observed,
        kappa,
        note,
    })
}
