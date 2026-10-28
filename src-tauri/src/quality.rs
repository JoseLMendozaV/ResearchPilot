//! Quality assessment (sheet Calidad): configurable questions Q1..Qn with a
//! scale, weight and description; per-reviewer scores on full text; totals
//! validated against the scale; missing-answer rule and threshold; consensus
//! or conflict when two reviewers assess the same study.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    db::{Database, now},
    error::{AppError, AppResult},
    review::{Actor, active_reviewer, audit, ensure_project},
};

/// Score and justification per criterion id.
type Scores = HashMap<String, (f64, String)>;
type ScoresByActor = HashMap<String, Scores>;

/// Actor name of the final (consensus) scores.
const CONSENSUS: &str = "__consenso__";

fn require(condition: bool, message: &str) -> AppResult<()> {
    if condition {
        Ok(())
    } else {
        Err(AppError::Validation(message.into()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScaleOption {
    pub label: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityCriterion {
    pub id: String,
    pub code: String,
    pub question: String,
    pub description: String,
    pub weight: f64,
    pub scale: Vec<ScaleOption>,
    pub max_score: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityCriterionInput {
    pub id: Option<String>,
    pub project_id: String,
    pub code: Option<String>,
    pub question: String,
    pub description: String,
    pub weight: f64,
    pub scale: Vec<ScaleOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreEntry {
    pub criterion_id: String,
    /// `None` = not answered.
    pub score: Option<f64>,
    #[serde(default)]
    pub justification: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    pub reviewer_name: String,
    pub scores: Vec<ScoreEntry>,
    pub total: f64,
    pub max_total: f64,
    pub percent: Option<f64>,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityRow {
    pub article_id: String,
    pub sequence: i64,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub venue: Option<String>,
    pub stage: String,
    pub reviews: Vec<Assessment>,
    pub final_assessment: Option<Assessment>,
    pub meets_threshold: Option<bool>,
    pub conflict_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityTable {
    pub criteria: Vec<QualityCriterion>,
    pub rows: Vec<QualityRow>,
    /// Highest total the scale allows (Σ weight × max value).
    pub max_total: f64,
    pub threshold: Option<f64>,
    pub missing_policy: String,
}

impl Database {
    pub fn save_quality_criterion(
        &self,
        input: QualityCriterionInput,
    ) -> AppResult<Vec<QualityCriterion>> {
        let question = input.question.trim();
        require(
            (5..=1000).contains(&question.chars().count()),
            "La pregunta de calidad debe tener entre 5 y 1000 caracteres",
        )?;
        require(
            input.weight.is_finite() && input.weight >= 0.0 && input.weight <= 100.0,
            "El peso debe estar entre 0 y 100",
        )?;
        require(
            (2..=10).contains(&input.scale.len()),
            "La escala necesita entre 2 y 10 opciones",
        )?;
        require(
            input.scale.iter().all(|option| {
                !option.label.trim().is_empty() && option.value.is_finite() && option.value >= 0.0
            }),
            "Cada opción de la escala necesita etiqueta y un valor no negativo",
        )?;
        let mut values: Vec<f64> = input.scale.iter().map(|option| option.value).collect();
        values.sort_by(f64::total_cmp);
        values.dedup();
        require(
            values.len() == input.scale.len(),
            "Los valores de la escala no pueden repetirse",
        )?;
        let max_score = *values.last().expect("non-empty scale");
        let connection = self.open()?;
        ensure_project(&connection, &input.project_id)?;
        let actor = active_reviewer(&connection, &input.project_id)?;
        let scale = serde_json::to_string(&input.scale)
            .map_err(|error| AppError::Validation(error.to_string()))?;
        match &input.id {
            Some(id) => {
                let scored: bool = connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM quality_scores WHERE criterion_id = ?1)",
                    [id],
                    |row| row.get(0),
                )?;
                let previous: String = connection.query_row(
                    "SELECT scale_json FROM quality_criteria WHERE id = ?1",
                    [id],
                    |row| row.get(0),
                )?;
                require(
                    !scored || previous == scale,
                    "La escala no puede cambiar cuando ya hay evaluaciones; crea una pregunta nueva",
                )?;
                connection.execute(
                    "UPDATE quality_criteria SET question = ?1, description = ?2, weight = ?3, max_score = ?4, scale_json = ?5 WHERE id = ?6 AND project_id = ?7",
                    params![question, input.description.trim(), input.weight, max_score, scale, id, input.project_id],
                )?;
                audit(
                    &connection,
                    &input.project_id,
                    "quality_criterion",
                    id,
                    "updated",
                    &actor,
                    None,
                    json!({ "question": question, "weight": input.weight }),
                )?;
            }
            None => {
                let count: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM quality_criteria WHERE project_id = ?1",
                    [&input.project_id],
                    |row| row.get(0),
                )?;
                let code = input
                    .code
                    .clone()
                    .filter(|code| !code.trim().is_empty())
                    .unwrap_or_else(|| format!("Q{}", count + 1));
                let id = Uuid::new_v4().to_string();
                connection
                    .execute(
                        "INSERT INTO quality_criteria (id, project_id, code, question, description, weight, max_score, scale_json, display_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        params![id, input.project_id, code.trim(), question, input.description.trim(), input.weight, max_score, scale, count + 1],
                    )
                    .map_err(|_| AppError::Validation(format!("Ya existe la pregunta {code}")))?;
                audit(
                    &connection,
                    &input.project_id,
                    "quality_criterion",
                    &id,
                    "created",
                    &actor,
                    None,
                    json!({ "code": code, "question": question }),
                )?;
            }
        }
        quality_criteria(&connection, &input.project_id)
    }

    pub fn delete_quality_criterion(
        &self,
        project_id: &str,
        criterion_id: &str,
    ) -> AppResult<Vec<QualityCriterion>> {
        let connection = self.open()?;
        let actor = active_reviewer(&connection, project_id)?;
        let scored: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM quality_scores WHERE criterion_id = ?1)",
            [criterion_id],
            |row| row.get(0),
        )?;
        require(
            !scored,
            "La pregunta ya tiene evaluaciones y no puede eliminarse",
        )?;
        connection.execute(
            "DELETE FROM quality_criteria WHERE id = ?1 AND project_id = ?2",
            params![criterion_id, project_id],
        )?;
        audit(
            &connection,
            project_id,
            "quality_criterion",
            criterion_id,
            "deleted",
            &actor,
            None,
            json!({}),
        )?;
        quality_criteria(&connection, project_id)
    }

    pub fn quality_table(&self, project_id: &str) -> AppResult<QualityTable> {
        let connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let criteria = quality_criteria(&connection, project_id)?;
        let (threshold, policy): (Option<f64>, String) = connection.query_row(
            "SELECT quality_threshold, quality_missing_policy FROM review_projects WHERE id = ?1",
            [project_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let mut statement = connection.prepare(
            "SELECT a.id, r.sequence, a.title, a.authors_json, a.publication_year, a.venue, r.workflow_stage,
                    (SELECT id FROM reviewer_conflicts c WHERE c.project_id = r.project_id AND c.article_id = a.id AND c.conflict_type = 'quality' AND c.status = 'open' LIMIT 1)
             FROM review_articles r JOIN articles a ON a.id = r.article_id
             WHERE r.project_id = ?1 AND r.removed_at IS NULL AND r.workflow_stage IN ('eligible', 'included') ORDER BY r.sequence",
        )?;
        let mut rows: Vec<QualityRow> = statement
            .query_map([project_id], |row| {
                Ok(QualityRow {
                    article_id: row.get(0)?,
                    sequence: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    title: row.get(2)?,
                    authors: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                    year: row.get(4)?,
                    venue: row.get(5)?,
                    stage: row.get(6)?,
                    reviews: Vec::new(),
                    final_assessment: None,
                    meets_threshold: None,
                    conflict_id: row.get(7)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        for row in &mut rows {
            let mut by_actor = scores_by_actor(&connection, project_id, &row.article_id)?;
            row.final_assessment = by_actor
                .remove(CONSENSUS)
                .map(|scores| assess("Consenso", scores, &criteria, &policy));
            row.reviews = by_actor
                .into_iter()
                .map(|(name, scores)| assess(&name, scores, &criteria, &policy))
                .collect();
            row.reviews
                .sort_by(|left, right| left.reviewer_name.cmp(&right.reviewer_name));
            row.meets_threshold = match (&row.final_assessment, threshold) {
                (Some(assessment), Some(threshold)) => assessment
                    .percent
                    .map(|percent| percent + 1e-9 >= threshold),
                _ => None,
            };
        }
        let max_total = criteria
            .iter()
            .map(|criterion| criterion.weight * criterion.max_score)
            .sum();
        Ok(QualityTable {
            criteria,
            rows,
            max_total,
            threshold,
            missing_policy: policy,
        })
    }

    pub fn save_quality_scores(
        &self,
        project_id: &str,
        article_id: &str,
        scores: Vec<ScoreEntry>,
    ) -> AppResult<QualityTable> {
        let mut connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let transaction = connection.transaction()?;
        let stage: Option<String> = transaction
            .query_row("SELECT workflow_stage FROM review_articles WHERE project_id = ?1 AND article_id = ?2 AND removed_at IS NULL", params![project_id, article_id], |row| row.get(0))
            .optional()?;
        require(
            matches!(stage.as_deref(), Some("eligible" | "included")),
            "La calidad se evalúa sobre texto completo de estudios elegibles o incluidos",
        )?;
        let criteria = quality_criteria(&transaction, project_id)?;
        validate_scores(&scores, &criteria)?;
        let actor = active_reviewer(&transaction, project_id)?;
        let mode: String = transaction.query_row(
            "SELECT screening_mode FROM review_projects WHERE id = ?1",
            [project_id],
            |row| row.get(0),
        )?;
        write_scores(
            &transaction,
            article_id,
            &scores,
            &actor.name,
            Some(&actor.reviewer_id),
            false,
        )?;
        if mode == "single" {
            write_scores(&transaction, article_id, &scores, CONSENSUS, None, true)?;
        } else {
            reconcile(&transaction, project_id, article_id, &criteria)?;
        }
        audit(
            &transaction,
            project_id,
            "quality_assessment",
            article_id,
            "scored",
            &actor,
            None,
            json!({ "scores": scores }),
        )?;
        transaction.commit()?;
        self.quality_table(project_id)
    }

    pub fn resolve_quality_conflict(
        &self,
        conflict_id: &str,
        scores: Vec<ScoreEntry>,
        reason: &str,
    ) -> AppResult<QualityTable> {
        require(
            reason.trim().len() >= 3,
            "Explica la resolución del conflicto",
        )?;
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        let (project_id, article_id): (String, String) = transaction
            .query_row(
                "SELECT project_id, article_id FROM reviewer_conflicts WHERE id = ?1 AND status = 'open' AND conflict_type = 'quality'",
                [conflict_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::Validation("El conflicto no existe o ya fue resuelto".into()))?;
        let criteria = quality_criteria(&transaction, &project_id)?;
        validate_scores(&scores, &criteria)?;
        let actor: Actor = active_reviewer(&transaction, &project_id)?;
        write_scores(
            &transaction,
            &article_id,
            &scores,
            CONSENSUS,
            Some(&actor.reviewer_id),
            true,
        )?;
        transaction.execute(
            "UPDATE reviewer_conflicts SET status = 'resolved', resolution = ?1, resolver_id = ?2, resolved_at = ?3 WHERE id = ?4",
            params![reason.trim(), actor.reviewer_id, now(), conflict_id],
        )?;
        audit(
            &transaction,
            &project_id,
            "reviewer_conflict",
            conflict_id,
            "resolved",
            &actor,
            None,
            json!({ "scores": scores, "reason": reason.trim() }),
        )?;
        transaction.commit()?;
        self.quality_table(&project_id)
    }
}

fn quality_criteria(connection: &Connection, project_id: &str) -> AppResult<Vec<QualityCriterion>> {
    let mut statement = connection.prepare(
        "SELECT id, code, question, description, weight, scale_json, max_score FROM quality_criteria WHERE project_id = ?1 ORDER BY display_order, code",
    )?;
    Ok(statement
        .query_map([project_id], |row| {
            Ok(QualityCriterion {
                id: row.get(0)?,
                code: row.get(1)?,
                question: row.get(2)?,
                description: row.get(3)?,
                weight: row.get(4)?,
                scale: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                max_score: row.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

fn validate_scores(scores: &[ScoreEntry], criteria: &[QualityCriterion]) -> AppResult<()> {
    for entry in scores {
        let criterion = criteria
            .iter()
            .find(|criterion| criterion.id == entry.criterion_id)
            .ok_or_else(|| AppError::Validation("Pregunta de calidad desconocida".into()))?;
        if let Some(score) = entry.score {
            require(
                criterion
                    .scale
                    .iter()
                    .any(|option| (option.value - score).abs() < 1e-9),
                &format!(
                    "{}: el puntaje {score} no pertenece a la escala definida",
                    criterion.code
                ),
            )?;
        }
    }
    Ok(())
}

fn write_scores(
    connection: &Connection,
    article_id: &str,
    scores: &[ScoreEntry],
    actor_name: &str,
    reviewer_id: Option<&str>,
    is_final: bool,
) -> AppResult<()> {
    for entry in scores {
        match entry.score {
            Some(score) => {
                connection.execute(
                    "INSERT INTO quality_scores (id, criterion_id, article_id, score, justification, actor_type, actor_name, assessed_at, reviewer_id, is_final)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'human', ?6, ?7, ?8, ?9)
                     ON CONFLICT(criterion_id, article_id, actor_type, actor_name) DO UPDATE SET
                        score = excluded.score, justification = excluded.justification, assessed_at = excluded.assessed_at,
                        reviewer_id = excluded.reviewer_id, is_final = excluded.is_final",
                    params![Uuid::new_v4().to_string(), entry.criterion_id, article_id, score, entry.justification.trim(), actor_name, now(), reviewer_id, is_final],
                )?;
            }
            None => {
                connection.execute(
                    "DELETE FROM quality_scores WHERE criterion_id = ?1 AND article_id = ?2 AND actor_type = 'human' AND actor_name = ?3",
                    params![entry.criterion_id, article_id, actor_name],
                )?;
            }
        }
    }
    Ok(())
}

fn scores_by_actor(
    connection: &Connection,
    project_id: &str,
    article_id: &str,
) -> AppResult<ScoresByActor> {
    let mut statement = connection.prepare(
        "SELECT s.actor_name, s.criterion_id, s.score, s.justification FROM quality_scores s
         JOIN quality_criteria c ON c.id = s.criterion_id WHERE c.project_id = ?1 AND s.article_id = ?2",
    )?;
    let mut map: ScoresByActor = HashMap::new();
    for row in statement.query_map(params![project_id, article_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })? {
        let (actor, criterion, score, justification) = row?;
        map.entry(actor)
            .or_default()
            .insert(criterion, (score, justification));
    }
    Ok(map)
}

/// Weighted total; missing answers count as 0 or are removed from the maximum.
fn assess(name: &str, scores: Scores, criteria: &[QualityCriterion], policy: &str) -> Assessment {
    let mut total = 0.0;
    let mut max_total = 0.0;
    let mut entries = Vec::new();
    for criterion in criteria {
        match scores.get(&criterion.id) {
            Some((score, justification)) => {
                total += criterion.weight * score;
                max_total += criterion.weight * criterion.max_score;
                entries.push(ScoreEntry {
                    criterion_id: criterion.id.clone(),
                    score: Some(*score),
                    justification: justification.clone(),
                });
            }
            None => {
                if policy == "zero" {
                    max_total += criterion.weight * criterion.max_score;
                }
                entries.push(ScoreEntry {
                    criterion_id: criterion.id.clone(),
                    score: None,
                    justification: String::new(),
                });
            }
        }
    }
    Assessment {
        reviewer_name: name.to_owned(),
        complete: entries.iter().all(|entry| entry.score.is_some()),
        percent: (max_total > 0.0).then(|| total / max_total * 100.0),
        total,
        max_total,
        scores: entries,
    }
}

/// Two complete, identical assessments become the consensus; any difference opens a conflict.
fn reconcile(
    connection: &Connection,
    project_id: &str,
    article_id: &str,
    criteria: &[QualityCriterion],
) -> AppResult<()> {
    let mut by_actor = scores_by_actor(connection, project_id, article_id)?;
    let has_resolution: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM reviewer_conflicts WHERE project_id = ?1 AND article_id = ?2 AND conflict_type = 'quality' AND status = 'resolved' AND resolver_id IS NOT NULL)",
        params![project_id, article_id],
        |row| row.get(0),
    )?;
    by_actor.remove(CONSENSUS);
    let complete: Vec<(&String, &Scores)> = by_actor
        .iter()
        .filter(|(_, scores)| {
            criteria
                .iter()
                .all(|criterion| scores.contains_key(&criterion.id))
        })
        .collect();
    if complete.len() < 2 || has_resolution {
        return Ok(());
    }
    let open: Option<String> = connection
        .query_row(
            "SELECT id FROM reviewer_conflicts WHERE project_id = ?1 AND article_id = ?2 AND conflict_type = 'quality' AND status = 'open'",
            params![project_id, article_id],
            |row| row.get(0),
        )
        .optional()?;
    let reference = complete[0].1;
    let agree = complete.iter().all(|(_, scores)| {
        criteria
            .iter()
            .all(|criterion| (scores[&criterion.id].0 - reference[&criterion.id].0).abs() < 1e-9)
    });
    if agree {
        let consensus: Vec<ScoreEntry> = criteria
            .iter()
            .map(|criterion| ScoreEntry {
                criterion_id: criterion.id.clone(),
                score: Some(reference[&criterion.id].0),
                justification: "Acuerdo entre revisores".into(),
            })
            .collect();
        write_scores(connection, article_id, &consensus, CONSENSUS, None, true)?;
        if let Some(conflict) = open {
            connection.execute("UPDATE reviewer_conflicts SET status = 'resolved', resolution = 'Los revisores coincidieron', resolved_at = ?1 WHERE id = ?2", params![now(), conflict])?;
        }
    } else {
        connection.execute(
            "DELETE FROM quality_scores WHERE article_id = ?1 AND actor_name = ?2 AND criterion_id IN (SELECT id FROM quality_criteria WHERE project_id = ?3)",
            params![article_id, CONSENSUS, project_id],
        )?;
        let differing: Vec<&str> = criteria
            .iter()
            .filter(|criterion| {
                complete.iter().any(|(_, scores)| {
                    (scores[&criterion.id].0 - reference[&criterion.id].0).abs() >= 1e-9
                })
            })
            .map(|criterion| criterion.code.as_str())
            .collect();
        let details = json!({ "criteria": differing, "reviewers": complete.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>() });
        match open {
            Some(conflict) => {
                connection.execute(
                    "UPDATE reviewer_conflicts SET details_json = ?1 WHERE id = ?2",
                    params![details.to_string(), conflict],
                )?;
            }
            None => {
                connection.execute(
                    "INSERT INTO reviewer_conflicts (id, project_id, article_id, conflict_type, status, created_at, phase, details_json) VALUES (?1, ?2, ?3, 'quality', 'open', ?4, 'full_text', ?5)",
                    params![Uuid::new_v4().to_string(), project_id, article_id, now(), details.to_string()],
                )?;
            }
        }
    }
    Ok(())
}
