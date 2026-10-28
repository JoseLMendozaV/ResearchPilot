//! Extraction matrix (sheet Extraccion): rows are included studies, columns are
//! the General aspects followed by each RQ's aspects. A cell may hold several
//! values (several results or configurations per paper), each typed, with a
//! verification status, notes and links to literal evidence. Aspects mapped to
//! a master-record field can be prefilled from human-confirmed values.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    db::{Database, now},
    error::{AppError, AppResult},
    review::{ExtractionField, active_reviewer, audit, ensure_project},
};

/// (id, value_json, display, quote, page) of a confirmed master value.
type MasterValueRow = (String, String, String, Option<String>, Option<i64>);
/// (id, field_id, value_json, status, notes, actor_type, actor_name, updated_at, unit)
type ValueRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
);

const STATUSES: [&str; 7] = [
    "confirmed",
    "proposed",
    "not_reported",
    "not_found",
    "not_applicable",
    "needs_review",
    "rejected",
];

fn require(condition: bool, message: &str) -> AppResult<()> {
    if condition {
        Ok(())
    } else {
        Err(AppError::Validation(message.into()))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellEvidence {
    pub id: String,
    pub quote: String,
    pub page: Option<i64>,
    pub bbox: Option<Vec<f64>>,
    pub page_width: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellValue {
    pub id: String,
    pub value: Value,
    pub display: String,
    pub status: String,
    pub notes: String,
    pub actor_type: String,
    pub actor_name: String,
    pub updated_at: String,
    pub evidence: Vec<CellEvidence>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatrixGroup {
    /// `general` or the research question id.
    pub key: String,
    pub label: String,
    pub question: Option<String>,
    pub fields: Vec<ExtractionField>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatrixRow {
    pub article_id: String,
    pub sequence: i64,
    pub reference: String,
    pub title: String,
    pub year: Option<i64>,
    pub stage: String,
    pub cells: HashMap<String, Vec<CellValue>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionMatrix {
    pub groups: Vec<MatrixGroup>,
    pub rows: Vec<MatrixRow>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellInput {
    pub project_id: String,
    pub article_id: String,
    pub field_id: String,
    /// Update this value instead of adding a new one.
    pub value_id: Option<String>,
    pub value_text: Option<String>,
    pub status: String,
    pub notes: String,
    /// Link existing evidence (e.g. from the master record).
    pub evidence_id: Option<String>,
    pub quote: Option<String>,
    pub page: Option<i64>,
}

/// "Author et al. (Year)" for the Reference column.
pub fn short_reference(authors: &[String], year: Option<i64>) -> String {
    let surname = |name: &str| -> String {
        let name = name.trim();
        match name.split_once(',') {
            Some((last, _)) => last.trim().to_owned(),
            None => name.split_whitespace().last().unwrap_or(name).to_owned(),
        }
    };
    let who = match authors.len() {
        0 => "Sin autor".to_owned(),
        1 => surname(&authors[0]),
        2 => format!("{} y {}", surname(&authors[0]), surname(&authors[1])),
        _ => format!("{} et al.", surname(&authors[0])),
    };
    format!(
        "{who} ({})",
        year.map_or("s. f.".to_owned(), |year| year.to_string())
    )
}

/// Parse a typed cell value; categories must exist in the aspect's taxonomy.
pub fn parse_cell(field: &ExtractionField, text: &str) -> AppResult<(Value, String)> {
    let text = text.trim();
    if text.is_empty() {
        return Ok((Value::Null, String::new()));
    }
    Ok(match field.data_type.as_str() {
        "number" => {
            let number: f64 = text.replace(',', ".").parse().map_err(|_| {
                AppError::Validation(format!("{}: el valor debe ser numérico", field.name))
            })?;
            let display = match &field.unit {
                Some(unit) if !unit.is_empty() => format!("{text} {unit}"),
                _ => text.to_owned(),
            };
            (json!(number), display)
        }
        "boolean" => match text.to_lowercase().as_str() {
            "sí" | "si" | "yes" | "true" | "1" => (json!(true), "Sí".into()),
            "no" | "false" | "0" => (json!(false), "No".into()),
            _ => {
                return Err(AppError::Validation(format!(
                    "{}: responde sí o no",
                    field.name
                )));
            }
        },
        "date" => {
            let valid = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok()
                || (text.len() == 4 && text.parse::<i32>().is_ok());
            require(valid, &format!("{}: usa AAAA-MM-DD o un año", field.name))?;
            (json!(text), text.to_owned())
        }
        "category" => {
            let category = field
                .taxonomy
                .iter()
                .find(|category| category.eq_ignore_ascii_case(text))
                .ok_or_else(|| {
                    AppError::Validation(format!(
                        "{}: «{text}» no está en la taxonomía ({})",
                        field.name,
                        field.taxonomy.join(", ")
                    ))
                })?;
            (json!(category), category.clone())
        }
        "list" => {
            let items: Vec<String> = text
                .split(';')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect();
            let display = items.join("; ");
            (json!(items), display)
        }
        _ => (json!(text), text.to_owned()),
    })
}

impl Database {
    pub fn extraction_matrix(
        &self,
        project_id: &str,
        include_eligible: bool,
    ) -> AppResult<ExtractionMatrix> {
        let overview = self.research_questions(project_id)?;
        let connection = self.open()?;
        let mut groups = vec![MatrixGroup {
            key: "general".into(),
            label: "General".into(),
            question: None,
            fields: overview.general_fields.clone(),
        }];
        groups.extend(overview.questions.iter().map(|question| MatrixGroup {
            key: question.id.clone(),
            label: question.code.clone(),
            question: Some(question.question.clone()),
            fields: question.fields.clone(),
        }));
        let stages = if include_eligible {
            "('included', 'eligible')"
        } else {
            "('included')"
        };
        let mut statement = connection.prepare(&format!(
            "SELECT a.id, r.sequence, a.title, a.authors_json, a.publication_year, r.workflow_stage
             FROM review_articles r JOIN articles a ON a.id = r.article_id
             WHERE r.project_id = ?1 AND r.removed_at IS NULL AND r.workflow_stage IN {stages} AND r.is_secondary = 0 ORDER BY r.sequence"
        ))?;
        let mut rows: Vec<MatrixRow> = statement
            .query_map([project_id], |row| {
                let authors: Vec<String> =
                    serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default();
                let year: Option<i64> = row.get(4)?;
                Ok(MatrixRow {
                    article_id: row.get(0)?,
                    sequence: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    reference: short_reference(&authors, year),
                    title: row.get(2)?,
                    year,
                    stage: row.get(5)?,
                    cells: HashMap::new(),
                })
            })?
            .collect::<Result<_, _>>()?;
        for row in &mut rows {
            row.cells = cells(&connection, project_id, &row.article_id)?;
        }
        Ok(ExtractionMatrix { groups, rows })
    }

    pub fn save_extraction_value(&self, input: CellInput) -> AppResult<Vec<CellValue>> {
        require(
            STATUSES.contains(&input.status.as_str()),
            "Estado de verificación no reconocido",
        )?;
        let field = self
            .research_questions(&input.project_id)?
            .questions
            .into_iter()
            .flat_map(|question| question.fields)
            .chain(self.research_questions(&input.project_id)?.general_fields)
            .find(|field| field.id == input.field_id)
            .ok_or_else(|| {
                AppError::Validation("El aspecto no pertenece a esta revisión".into())
            })?;
        let (value, _) = parse_cell(&field, input.value_text.as_deref().unwrap_or(""))?;
        require(
            !value.is_null()
                || !["confirmed", "proposed", "needs_review"].contains(&input.status.as_str()),
            "Ese estado necesita un valor; usa «No reportado», «No encontrado» o «No aplica» si no hay dato",
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
        let actor = active_reviewer(&transaction, &input.project_id)?;
        let timestamp = now();
        let value_json = value.to_string();
        let value_id = match &input.value_id {
            Some(id) => {
                let before: Option<(String, String)> = transaction
                    .query_row("SELECT normalized_value_json, status FROM extracted_values WHERE id = ?1 AND field_id = ?2 AND article_id = ?3", params![id, input.field_id, input.article_id], |row| Ok((row.get(0)?, row.get(1)?)))
                    .optional()?;
                let (before_value, before_status) =
                    before.ok_or_else(|| AppError::Validation("El valor no existe".into()))?;
                transaction.execute(
                    "UPDATE extracted_values SET normalized_value_json = ?1, raw_quote = COALESCE(?2, raw_quote), page = COALESCE(?3, page), status = ?4,
                            notes = ?5, reviewer_id = ?6, updated_at = ?7, unit = ?8 WHERE id = ?9",
                    params![value_json, input.quote.as_deref().map(str::trim).filter(|quote| !quote.is_empty()), input.page, input.status, input.notes.trim(), actor.reviewer_id, timestamp, field.unit, id],
                )?;
                audit(
                    &transaction,
                    &input.project_id,
                    "extracted_value",
                    id,
                    "updated",
                    &actor,
                    Some(json!({ "value": before_value, "status": before_status })),
                    json!({ "value": value, "status": input.status, "notes": input.notes.trim() }),
                )?;
                id.clone()
            }
            None => {
                let id = Uuid::new_v4().to_string();
                transaction.execute(
                    "INSERT INTO extracted_values (id, field_id, article_id, raw_quote, normalized_value_json, value_type, unit, page, status, reviewer_id, notes,
                                                   actor_type, actor_name, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'human', ?12, ?13, ?13)",
                    params![id, input.field_id, input.article_id, input.quote.as_deref().map(str::trim).filter(|quote| !quote.is_empty()), value_json, field.data_type, field.unit, input.page, input.status, actor.reviewer_id, input.notes.trim(), actor.name, timestamp],
                )?;
                audit(
                    &transaction,
                    &input.project_id,
                    "extracted_value",
                    &id,
                    "created",
                    &actor,
                    None,
                    json!({ "field": field.name, "value": value, "status": input.status }),
                )?;
                id
            }
        };
        if let Some(evidence_id) = &input.evidence_id {
            let same_article: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM evidence_spans WHERE id = ?1 AND article_id = ?2)",
                params![evidence_id, input.article_id],
                |row| row.get(0),
            )?;
            require(same_article, "La evidencia no pertenece a este artículo")?;
            transaction.execute("INSERT OR IGNORE INTO extraction_value_evidence (extracted_value_id, evidence_id) VALUES (?1, ?2)", params![value_id, evidence_id])?;
        } else if let Some(quote) = input
            .quote
            .as_deref()
            .map(str::trim)
            .filter(|quote| !quote.is_empty())
        {
            let evidence_id = Uuid::new_v4().to_string();
            let verified: bool = match input.page {
                Some(page) => transaction
                    .query_row(
                        "SELECT instr(lower(p.text_content), lower(?1)) > 0 FROM document_pages p JOIN document_files d ON d.id = p.document_file_id
                         WHERE d.article_id = ?2 AND d.is_primary = 1 AND d.deleted_at IS NULL AND p.page_number = ?3",
                        params![quote, input.article_id, page],
                        |row| row.get(0),
                    )
                    .optional()?
                    .unwrap_or(false),
                None => false,
            };
            transaction.execute(
                "INSERT INTO evidence_spans (id, article_id, page_start, page_end, quoted_text, locator_json, created_at) VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6)",
                params![evidence_id, input.article_id, input.page, quote, json!({ "method": "manual-matrix", "verified": verified }).to_string(), timestamp],
            )?;
            transaction.execute("INSERT INTO extraction_value_evidence (extracted_value_id, evidence_id) VALUES (?1, ?2)", params![value_id, evidence_id])?;
        }
        transaction.commit()?;
        Ok(cells(&self.open()?, &input.project_id, &input.article_id)?
            .remove(&input.field_id)
            .unwrap_or_default())
    }

    /// Copy human-confirmed master-record values into empty cells of mapped aspects,
    /// as proposals that keep the original evidence. Returns how many were copied.
    pub fn prefill_matrix_from_master(&self, project_id: &str) -> AppResult<i64> {
        let matrix = self.extraction_matrix(project_id, true)?;
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        let actor = active_reviewer(&transaction, project_id)?;
        let mut copied = 0;
        for field in matrix
            .groups
            .iter()
            .flat_map(|group| &group.fields)
            .filter(|field| field.master_field_key.is_some())
        {
            let key = field.master_field_key.as_deref().expect("mapped");
            for row in &matrix.rows {
                if row
                    .cells
                    .get(&field.id)
                    .is_some_and(|values| !values.is_empty())
                {
                    continue;
                }
                let mut statement = transaction.prepare(
                    "SELECT id, value_json, display_value, raw_quote, page FROM master_record_values
                     WHERE article_id = ?1 AND field_key = ?2 AND status = 'confirmed' AND superseded_by_run_id IS NULL",
                )?;
                let values: Vec<MasterValueRow> = statement
                    .query_map(params![row.article_id, key], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    })?
                    .collect::<Result<_, _>>()?;
                for (master_id, value_json, display, quote, page) in values {
                    let id = Uuid::new_v4().to_string();
                    let timestamp = now();
                    transaction.execute(
                        "INSERT INTO extracted_values (id, field_id, article_id, raw_quote, normalized_value_json, value_type, unit, page, status, notes,
                                                       actor_type, actor_name, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'proposed', ?9, 'import', 'Ficha maestra', ?10, ?10)",
                        params![id, field.id, row.article_id, quote, value_json, field.data_type, field.unit, page, format!("Copiado de la ficha maestra (valor confirmado): {display}"), timestamp],
                    )?;
                    transaction.execute(
                        "INSERT OR IGNORE INTO extraction_value_evidence (extracted_value_id, evidence_id) SELECT ?1, evidence_id FROM master_value_evidence WHERE value_id = ?2",
                        params![id, master_id],
                    )?;
                    copied += 1;
                }
            }
        }
        if copied > 0 {
            audit(
                &transaction,
                project_id,
                "extraction_matrix",
                project_id,
                "prefilled_from_master_record",
                &actor,
                None,
                json!({ "values": copied }),
            )?;
        }
        transaction.commit()?;
        Ok(copied)
    }
}

fn cells(
    connection: &Connection,
    project_id: &str,
    article_id: &str,
) -> AppResult<HashMap<String, Vec<CellValue>>> {
    let mut statement = connection.prepare(
        "SELECT v.id, v.field_id, v.normalized_value_json, v.status, v.notes, v.actor_type, v.actor_name, v.updated_at, v.unit
         FROM extracted_values v JOIN extraction_fields f ON f.id = v.field_id
         WHERE f.project_id = ?1 AND v.article_id = ?2 AND f.deleted_at IS NULL ORDER BY v.created_at",
    )?;
    let mut evidence = connection.prepare(
        "SELECT e.id, e.quoted_text, e.page_start, e.locator_json, p.width FROM extraction_value_evidence link
         JOIN evidence_spans e ON e.id = link.evidence_id
         LEFT JOIN document_pages p ON p.document_file_id = e.document_file_id AND p.page_number = e.page_start
         WHERE link.extracted_value_id = ?1",
    )?;
    let mut map: HashMap<String, Vec<CellValue>> = HashMap::new();
    let rows: Vec<ValueRow> = statement
        .query_map(params![project_id, article_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    for (id, field_id, value_json, status, notes, actor_type, actor_name, updated_at, unit) in rows
    {
        let value: Value = serde_json::from_str(&value_json).unwrap_or(Value::Null);
        let display = match &value {
            Value::Null => String::new(),
            Value::String(text) => text.clone(),
            Value::Bool(flag) => {
                if *flag {
                    "Sí".into()
                } else {
                    "No".into()
                }
            }
            Value::Array(items) => items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("; "),
            Value::Number(number) => {
                let number = match number.as_f64() {
                    Some(float) if float.fract() == 0.0 && float.abs() < 1e15 => {
                        format!("{}", float as i64)
                    }
                    _ => number.to_string(),
                };
                match &unit {
                    Some(unit) if !unit.is_empty() => format!("{number} {unit}"),
                    _ => number,
                }
            }
            other => other.to_string(),
        };
        let items = evidence
            .query_map([&id], |row| {
                let locator: Value =
                    serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or(Value::Null);
                Ok(CellEvidence {
                    id: row.get(0)?,
                    quote: row.get(1)?,
                    page: row.get(2)?,
                    bbox: locator["bbox"]
                        .as_array()
                        .map(|items| items.iter().filter_map(Value::as_f64).collect()),
                    page_width: row
                        .get::<_, Option<f64>>(4)?
                        .or_else(|| locator["pageWidth"].as_f64()),
                })
            })?
            .collect::<Result<_, _>>()?;
        map.entry(field_id).or_default().push(CellValue {
            id,
            value,
            display,
            status,
            notes,
            actor_type,
            actor_name,
            updated_at,
            evidence: items,
        });
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(data_type: &str, taxonomy: &[&str]) -> ExtractionField {
        ExtractionField {
            id: "f".into(),
            research_question_id: None,
            name: "Aspecto".into(),
            description: String::new(),
            data_type: data_type.into(),
            unit: Some("%".into()),
            taxonomy: taxonomy.iter().map(|item| item.to_string()).collect(),
            instructions: String::new(),
            required: false,
            examples: Vec::new(),
            master_field_key: None,
            display_order: 0,
        }
    }

    #[test]
    fn typed_cells_are_validated() {
        assert_eq!(
            parse_cell(&field("number", &[]), "93,2").unwrap(),
            (json!(93.2), "93,2 %".into())
        );
        assert!(parse_cell(&field("number", &[]), "alto").is_err());
        assert_eq!(
            parse_cell(
                &field("category", &["Supervisado", "Semisupervisado"]),
                "semisupervisado"
            )
            .unwrap()
            .1,
            "Semisupervisado"
        );
        assert!(parse_cell(&field("category", &["Supervisado"]), "Otro").is_err());
        assert_eq!(
            parse_cell(&field("boolean", &[]), "Sí").unwrap().0,
            json!(true)
        );
        assert_eq!(
            parse_cell(&field("list", &[]), "a; b;").unwrap().0,
            json!(["a", "b"])
        );
        assert!(parse_cell(&field("date", &[]), "ayer").is_err());
    }

    #[test]
    fn references_are_short_and_honest_about_missing_data() {
        assert_eq!(
            short_reference(
                &[
                    "Researcher, Ana".into(),
                    "Luis Scientist".into(),
                    "X".into()
                ],
                Some(2024)
            ),
            "Researcher et al. (2024)"
        );
        assert_eq!(
            short_reference(&["Ana Researcher".into(), "Luis Scientist".into()], None),
            "Researcher y Scientist (s. f.)"
        );
        assert_eq!(short_reference(&[], Some(2020)), "Sin autor (2020)");
    }
}
