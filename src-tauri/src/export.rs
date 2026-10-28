//! Project exports (Phase 7): the seven-sheet evidence workbook (compatible and
//! extended with provenance), CSV matrix, JSON snapshot, RIS, BibTeX, Markdown
//! and LaTeX reports with an evidence annex. Every export is recorded with its
//! SHA-256 so a reported table can be traced to the exact file produced.

use std::{
    collections::HashMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use rusqlite::params;
use rust_xlsxwriter::{Color, Format, Workbook, Worksheet, XlsxError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    db::{Database, now},
    error::{AppError, AppResult},
    library::sha256_file,
    matrix::{CellValue, ExtractionMatrix, short_reference},
    quality::QualityTable,
    review::{
        AuditEntry, ControlCoverage, ProjectWorkspace, QuestionsOverview, SearchStrategy,
        active_reviewer, audit,
    },
    screening::{ScreeningRow, ScreeningTable},
};

pub const SNAPSHOT_FORMAT: &str = "researchpilot-project-export/1";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub project_id: String,
    /// `xlsx`, `csv`, `json`, `ris`, `bibtex`, `markdown` or `latex`.
    pub format: String,
    /// For `xlsx`: `compatible` (seven sheets only) or `extended` (with provenance).
    #[serde(default)]
    pub variant: Option<String>,
    /// For bibliographies: `included` (default) or `corpus`.
    #[serde(default)]
    pub scope: Option<String>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub id: String,
    pub path: String,
    pub format: String,
    pub bytes: u64,
    pub sha256: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRecord {
    pub id: String,
    pub format: String,
    pub output_path: String,
    pub output_sha256: Option<String>,
    pub options: Value,
    pub created_at: String,
}

/// Everything a project export needs, gathered once.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
    pub format: &'static str,
    pub app_version: &'static str,
    pub generated_at: String,
    pub workspace: ProjectWorkspace,
    pub control: ControlCoverage,
    pub questions: QuestionsOverview,
    pub strategies: Vec<SearchStrategy>,
    pub screening_metadata: ScreeningTable,
    pub screening_full_text: ScreeningTable,
    pub quality: QualityTable,
    pub matrix: ExtractionMatrix,
    pub audit: Vec<AuditEntry>,
}

fn extension(format: &str) -> AppResult<&'static str> {
    Ok(match format {
        "xlsx" => "xlsx",
        "csv" => "csv",
        "json" => "json",
        "ris" => "ris",
        "bibtex" => "bib",
        "markdown" => "md",
        "latex" => "tex",
        _ => {
            return Err(AppError::Validation(
                "Formato de exportación no reconocido".into(),
            ));
        }
    })
}

fn xlsx_error(error: XlsxError) -> AppError {
    AppError::Validation(format!("No se pudo generar el Excel: {error}"))
}

impl Database {
    pub fn project_snapshot(&self, project_id: &str) -> AppResult<ProjectSnapshot> {
        Ok(ProjectSnapshot {
            format: SNAPSHOT_FORMAT,
            app_version: env!("CARGO_PKG_VERSION"),
            generated_at: now(),
            workspace: self.project_workspace(project_id)?,
            control: self.control_coverage(project_id)?,
            questions: self.research_questions(project_id)?,
            strategies: self.search_strategies(project_id)?,
            screening_metadata: self.screening_table(project_id, "metadata")?,
            screening_full_text: self.screening_table(project_id, "full_text")?,
            quality: self.quality_table(project_id)?,
            matrix: self.extraction_matrix(project_id, false)?,
            audit: self.project_audit(project_id, 2000)?,
        })
    }

    pub fn export_project(&self, request: ExportRequest) -> AppResult<ExportResult> {
        let extension = extension(&request.format)?;
        let mut path = PathBuf::from(request.path.trim());
        if !path.is_absolute() {
            return Err(AppError::Validation(
                "Elige la ubicación del archivo con el diálogo de guardado".into(),
            ));
        }
        if path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_lowercase)
            .as_deref()
            != Some(extension)
        {
            path.set_extension(extension);
        }
        if !path.parent().is_some_and(Path::is_dir) {
            return Err(AppError::Validation(
                "La carpeta de destino no existe".into(),
            ));
        }
        let snapshot = self.project_snapshot(&request.project_id)?;
        let variant = request
            .variant
            .clone()
            .unwrap_or_else(|| "compatible".into());
        let scope = request.scope.clone().unwrap_or_else(|| "included".into());
        let summary = match request.format.as_str() {
            "xlsx" => write_workbook(&snapshot, variant == "extended", &path)?,
            "csv" => write_text(
                &path,
                &matrix_csv(&snapshot)?,
                "Matriz de extracción en CSV",
            )?,
            "json" => write_text(
                &path,
                &serde_json::to_string_pretty(&snapshot)
                    .map_err(|error| AppError::Validation(error.to_string()))?,
                "Instantánea completa del proyecto",
            )?,
            "ris" => write_text(
                &path,
                &ris(&bibliography_rows(&snapshot, &scope)),
                "Bibliografía RIS",
            )?,
            "bibtex" => write_text(
                &path,
                &bibtex(&bibliography_rows(&snapshot, &scope)),
                "Bibliografía BibTeX",
            )?,
            "markdown" => write_text(
                &path,
                &markdown(&snapshot),
                "Informe Markdown con anexo de evidencias",
            )?,
            "latex" => write_text(
                &path,
                &latex(&snapshot),
                "Informe LaTeX con anexo de evidencias",
            )?,
            _ => unreachable!("validated above"),
        };
        let sha256 = sha256_file(&path)?;
        let bytes = fs::metadata(&path)?.len();
        let id = Uuid::new_v4().to_string();
        let options = json!({ "variant": variant, "scope": scope, "appVersion": env!("CARGO_PKG_VERSION"), "protocolVersion": snapshot.workspace.protocol.first().map(|version| version.version_number) });
        let connection = self.open()?;
        connection.execute(
            "INSERT INTO export_records (id, project_id, format, output_path, output_sha256, options_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, request.project_id, request.format, path.to_string_lossy(), sha256, options.to_string(), now()],
        )?;
        let actor = active_reviewer(&connection, &request.project_id)?;
        audit(
            &connection,
            &request.project_id,
            "export",
            &id,
            "exported",
            &actor,
            None,
            json!({ "format": request.format, "sha256": sha256, "options": options }),
        )?;
        Ok(ExportResult {
            id,
            path: path.to_string_lossy().into_owned(),
            format: request.format,
            bytes,
            sha256,
            summary,
        })
    }

    pub fn export_history(&self, project_id: &str) -> AppResult<Vec<ExportRecord>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT id, format, output_path, output_sha256, options_json, created_at FROM export_records WHERE project_id = ?1 ORDER BY created_at DESC LIMIT 100",
        )?;
        Ok(statement
            .query_map([project_id], |row| {
                Ok(ExportRecord {
                    id: row.get(0)?,
                    format: row.get(1)?,
                    output_path: row.get(2)?,
                    output_sha256: row.get(3)?,
                    options: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or(Value::Null),
                    created_at: row.get(5)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }
}

fn write_text(path: &Path, content: &str, summary: &str) -> AppResult<String> {
    fs::write(path, content)?;
    Ok(summary.into())
}

// --- Shared text helpers ------------------------------------------------------------

const STATUS_TEXT: [(&str, &str); 7] = [
    ("confirmed", "Confirmado"),
    ("proposed", "Propuesto"),
    ("needs_review", "Por revisar"),
    ("not_reported", "No reportado"),
    ("not_found", "No encontrado"),
    ("not_applicable", "No aplica"),
    ("rejected", "Rechazado"),
];

fn status_text(status: &str) -> &str {
    STATUS_TEXT
        .iter()
        .find(|(key, _)| *key == status)
        .map_or(status, |(_, label)| label)
}

/// Text of a matrix cell: values joined with "; ", rejected values left out,
/// statuses without a value spelled out (e.g. "No reportado").
pub fn cell_text(values: &[CellValue]) -> String {
    values
        .iter()
        .filter(|value| value.status != "rejected")
        .map(|value| {
            if value.display.is_empty() {
                status_text(&value.status).to_owned()
            } else {
                value.display.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// "Researcher, Ana; Scientist, Luis (2024). Title. Venue. https://doi.org/…"
pub fn full_reference(row: &ScreeningRow) -> String {
    let mut text = String::new();
    text.push_str(if row.authors.is_empty() {
        "Sin autor"
    } else {
        ""
    });
    text.push_str(&row.authors.join("; "));
    let _ = write!(
        text,
        " ({}). {}.",
        row.year.map_or("s. f.".to_owned(), |year| year.to_string()),
        row.title.trim_end_matches('.')
    );
    if let Some(venue) = row.venue.as_deref().filter(|venue| !venue.is_empty()) {
        let _ = write!(text, " {venue}.");
    }
    if let Some(doi) = row.doi.as_deref().filter(|doi| !doi.is_empty()) {
        let _ = write!(text, " https://doi.org/{doi}");
    }
    text
}

fn decision_letter(row: &ScreeningRow) -> &'static str {
    match row
        .final_decision
        .as_ref()
        .map(|decision| decision.decision.as_str())
    {
        Some("include" | "awaiting_full_text") => "I",
        Some("exclude") => "E",
        Some(_) => "?",
        None => "",
    }
}

/// Criterion marks shown for an article: the final decision's, else the first reviewer's.
fn marks(row: &ScreeningRow) -> HashMap<String, String> {
    let source = row
        .final_decision
        .iter()
        .chain(row.reviews.iter())
        .map(|decision| &decision.criteria)
        .find(|criteria| {
            criteria
                .as_object()
                .is_some_and(|object| !object.is_empty())
        });
    source
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        match value.as_str() {
                            Some("met") => "Sí",
                            Some("not_met") => "No",
                            _ => "?",
                        }
                        .to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn aspect_columns(snapshot: &ProjectSnapshot) -> Vec<(&str, &crate::review::ExtractionField)> {
    snapshot
        .matrix
        .groups
        .iter()
        .flat_map(|group| {
            group
                .fields
                .iter()
                .map(move |field| (group.label.as_str(), field))
        })
        .collect()
}

fn included_rows(snapshot: &ProjectSnapshot) -> Vec<&ScreeningRow> {
    snapshot
        .screening_metadata
        .rows
        .iter()
        .filter(|row| row.stage == "included")
        .collect()
}

// --- XLSX -------------------------------------------------------------------------------

struct Styles {
    header: Format,
    group: Format,
    wrap: Format,
    title: Format,
}

fn styles() -> Styles {
    Styles {
        header: Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0xE2F0EE)),
        group: Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0xCCE7E3)),
        wrap: Format::new().set_text_wrap(),
        title: Format::new().set_bold().set_font_size(14),
    }
}

fn header(sheet: &mut Worksheet, row: u32, titles: &[&str], format: &Format) -> AppResult<()> {
    for (column, title) in titles.iter().enumerate() {
        sheet
            .write_string_with_format(row, column as u16, *title, format)
            .map_err(xlsx_error)?;
    }
    Ok(())
}

fn put(sheet: &mut Worksheet, row: u32, column: u16, text: &str) -> AppResult<()> {
    if !text.is_empty() {
        // Excel cells hold at most 32,767 characters.
        let text: String = text.chars().take(32_000).collect();
        sheet.write_string(row, column, text).map_err(xlsx_error)?;
    }
    Ok(())
}

fn write_workbook(snapshot: &ProjectSnapshot, extended: bool, path: &Path) -> AppResult<String> {
    let mut workbook = Workbook::new();
    let style = styles();
    let project = &snapshot.workspace.project;

    // Leer
    {
        let sheet = workbook
            .add_worksheet()
            .set_name("Leer")
            .map_err(xlsx_error)?;
        sheet.set_column_width(0, 28).map_err(xlsx_error)?;
        sheet.set_column_width(1, 100).map_err(xlsx_error)?;
        sheet
            .write_string_with_format(
                0,
                0,
                format!("ResearchPilot — {}", project.name),
                &style.title,
            )
            .map_err(xlsx_error)?;
        let protocol = snapshot.workspace.protocol.first();
        let lines = [
            ("Tipo de revisión", project.review_type.replace('_', " ")),
            ("Generado", snapshot.generated_at.clone()),
            ("Versión de ResearchPilot", snapshot.app_version.to_owned()),
            ("Protocolo", protocol.map_or("Sin protocolo".into(), |version| format!("versión {} ({})", version.version_number, version.status))),
            ("Estudios en el corpus", snapshot.workspace.counts.total.to_string()),
            ("Estudios incluidos", snapshot.workspace.counts.included.to_string()),
            ("Guía", "Mapeo o revisión sistemática: definir RQs y protocolo, validar cadenas con artículos de control, cribar por metadatos y luego texto completo, evaluar calidad y extraer datos por RQ. Ver Petersen, Vakkalanka y Kuzniarz (2015).".into()),
            ("Nota", "Los valores provienen de decisiones y extracciones registradas en ResearchPilot. La versión ampliada incluye estados de verificación, citas y páginas.".into()),
        ];
        for (index, (label, value)) in lines.iter().enumerate() {
            sheet
                .write_string_with_format(index as u32 + 2, 0, *label, &style.header)
                .map_err(xlsx_error)?;
            sheet
                .write_string_with_format(index as u32 + 2, 1, value, &style.wrap)
                .map_err(xlsx_error)?;
        }
    }

    // Control
    {
        let sheet = workbook
            .add_worksheet()
            .set_name("Control")
            .map_err(xlsx_error)?;
        let mut titles = vec!["Id", "Reference"];
        if extended {
            titles.extend(["DOI", "Justificación", "Recuperado por"]);
        }
        header(sheet, 0, &titles, &style.header)?;
        sheet.set_column_width(1, 90).map_err(xlsx_error)?;
        let by_article: HashMap<&str, &ScreeningRow> = snapshot
            .screening_metadata
            .rows
            .iter()
            .map(|row| (row.article_id.as_str(), row))
            .collect();
        for (index, control) in snapshot.control.articles.iter().enumerate() {
            let row = index as u32 + 1;
            sheet
                .write_number(row, 0, index as f64 + 1.0)
                .map_err(xlsx_error)?;
            let reference = by_article.get(control.article_id.as_str()).map_or_else(
                || {
                    format!(
                        "{} ({}).{}",
                        control.title,
                        control.year.map_or("s. f.".into(), |year| year.to_string()),
                        control
                            .doi
                            .as_ref()
                            .map_or(String::new(), |doi| format!(" https://doi.org/{doi}"))
                    )
                },
                |article| full_reference(article),
            );
            put(sheet, row, 1, &reference)?;
            if extended {
                put(sheet, row, 2, control.doi.as_deref().unwrap_or(""))?;
                put(sheet, row, 3, &control.rationale)?;
                put(
                    sheet,
                    row,
                    4,
                    &if control.found_by.is_empty() {
                        "No recuperado".into()
                    } else {
                        control.found_by.join(", ")
                    },
                )?;
            }
        }
    }

    // RQs
    {
        let sheet = workbook
            .add_worksheet()
            .set_name("RQs")
            .map_err(xlsx_error)?;
        let aspects = snapshot
            .questions
            .questions
            .iter()
            .map(|question| question.fields.len())
            .max()
            .unwrap_or(0)
            .max(1);
        let mut titles = vec!["RQ".to_owned()];
        titles.extend((1..=aspects).map(|number| format!("Aspecto {number}")));
        header(
            sheet,
            0,
            &titles.iter().map(String::as_str).collect::<Vec<_>>(),
            &style.header,
        )?;
        sheet.set_column_width(0, 70).map_err(xlsx_error)?;
        for (index, question) in snapshot.questions.questions.iter().enumerate() {
            let row = index as u32 + 1;
            put(
                sheet,
                row,
                0,
                &format!("{}: {}", question.code, question.question),
            )?;
            for (column, field) in question.fields.iter().enumerate() {
                put(sheet, row, column as u16 + 1, &field.name)?;
            }
        }
    }

    // Cadena
    {
        let sheet = workbook
            .add_worksheet()
            .set_name("Cadena")
            .map_err(xlsx_error)?;
        let mut titles = vec!["Base de datos", "Fecha", "Cadena", "Cantidad"];
        if extended {
            titles.extend(["Versión", "Registros importados", "Archivos (SHA-256)"]);
        }
        header(sheet, 0, &titles, &style.header)?;
        sheet.set_column_width(2, 80).map_err(xlsx_error)?;
        for (index, strategy) in snapshot.strategies.iter().enumerate() {
            let row = index as u32 + 1;
            put(sheet, row, 0, &strategy.database_name)?;
            put(sheet, row, 1, &strategy.query_date)?;
            sheet
                .write_string_with_format(row, 2, &strategy.query_text, &style.wrap)
                .map_err(xlsx_error)?;
            sheet
                .write_number(row, 3, strategy.result_count as f64)
                .map_err(xlsx_error)?;
            if extended {
                sheet
                    .write_number(row, 4, strategy.version_number as f64)
                    .map_err(xlsx_error)?;
                sheet
                    .write_number(row, 5, strategy.records_imported as f64)
                    .map_err(xlsx_error)?;
                put(
                    sheet,
                    row,
                    6,
                    &strategy
                        .imports
                        .iter()
                        .map(|item| item.source_sha256.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                )?;
            }
        }
    }

    // I_E
    {
        let sheet = workbook
            .add_worksheet()
            .set_name("I_E")
            .map_err(xlsx_error)?;
        let criteria: Vec<_> = snapshot
            .screening_metadata
            .criteria
            .iter()
            .filter(|criterion| criterion.active)
            .collect();
        let mut titles: Vec<String> = [
            "Id",
            "Reference",
            "Title",
            "Keywords",
            "Abstract",
            "Year",
            "Authors",
            "Conference/Journal",
        ]
        .iter()
        .map(|title| title.to_string())
        .collect();
        titles.extend(criteria.iter().map(|criterion| criterion.name.clone()));
        titles.push("I/E".into());
        header(
            sheet,
            0,
            &titles.iter().map(String::as_str).collect::<Vec<_>>(),
            &style.header,
        )?;
        sheet.set_column_width(2, 50).map_err(xlsx_error)?;
        sheet.set_column_width(4, 60).map_err(xlsx_error)?;
        sheet.set_freeze_panes(1, 0).map_err(xlsx_error)?;
        for (index, item) in snapshot.screening_metadata.rows.iter().enumerate() {
            let row = index as u32 + 1;
            sheet
                .write_number(row, 0, item.sequence as f64)
                .map_err(xlsx_error)?;
            put(sheet, row, 1, &short_reference(&item.authors, item.year))?;
            put(sheet, row, 2, &item.title)?;
            put(sheet, row, 3, &item.keywords.join("; "))?;
            put(sheet, row, 4, item.abstract_text.as_deref().unwrap_or(""))?;
            if let Some(year) = item.year {
                sheet
                    .write_number(row, 5, year as f64)
                    .map_err(xlsx_error)?;
            }
            put(sheet, row, 6, &item.authors.join("; "))?;
            put(sheet, row, 7, item.venue.as_deref().unwrap_or(""))?;
            let item_marks = marks(item);
            for (offset, criterion) in criteria.iter().enumerate() {
                put(
                    sheet,
                    row,
                    8 + offset as u16,
                    item_marks.get(&criterion.id).map_or("", String::as_str),
                )?;
            }
            put(sheet, row, 8 + criteria.len() as u16, decision_letter(item))?;
        }
    }

    // Calidad
    {
        let sheet = workbook
            .add_worksheet()
            .set_name("Calidad")
            .map_err(xlsx_error)?;
        let mut titles: Vec<String> = ["Id", "Reference", "Title", "Year"]
            .iter()
            .map(|title| title.to_string())
            .collect();
        titles.extend(
            snapshot
                .quality
                .criteria
                .iter()
                .map(|criterion| criterion.code.clone()),
        );
        titles.push("Total".into());
        header(
            sheet,
            0,
            &titles.iter().map(String::as_str).collect::<Vec<_>>(),
            &style.header,
        )?;
        sheet.set_column_width(2, 50).map_err(xlsx_error)?;
        for (index, item) in snapshot.quality.rows.iter().enumerate() {
            let row = index as u32 + 1;
            sheet
                .write_number(row, 0, item.sequence as f64)
                .map_err(xlsx_error)?;
            put(sheet, row, 1, &short_reference(&item.authors, item.year))?;
            put(sheet, row, 2, &item.title)?;
            if let Some(year) = item.year {
                sheet
                    .write_number(row, 3, year as f64)
                    .map_err(xlsx_error)?;
            }
            if let Some(final_assessment) = &item.final_assessment {
                for (offset, criterion) in snapshot.quality.criteria.iter().enumerate() {
                    if let Some(score) = final_assessment
                        .scores
                        .iter()
                        .find(|entry| entry.criterion_id == criterion.id)
                        .and_then(|entry| entry.score)
                    {
                        sheet
                            .write_number(row, 4 + offset as u16, score)
                            .map_err(xlsx_error)?;
                    }
                }
                sheet
                    .write_number(
                        row,
                        4 + snapshot.quality.criteria.len() as u16,
                        final_assessment.total,
                    )
                    .map_err(xlsx_error)?;
            }
        }
    }

    // Extraccion: group row (General / RQs) above the aspect row.
    {
        let sheet = workbook
            .add_worksheet()
            .set_name("Extraccion")
            .map_err(xlsx_error)?;
        for (column, title) in ["Id", "Reference", "Title", "Year"].iter().enumerate() {
            sheet
                .merge_range(0, column as u16, 1, column as u16, title, &style.header)
                .map_err(xlsx_error)?;
        }
        let mut column: u16 = 4;
        for group in snapshot
            .matrix
            .groups
            .iter()
            .filter(|group| !group.fields.is_empty())
        {
            let last = column + group.fields.len() as u16 - 1;
            if last > column {
                sheet
                    .merge_range(0, column, 0, last, &group.label, &style.group)
                    .map_err(xlsx_error)?;
            } else {
                sheet
                    .write_string_with_format(0, column, &group.label, &style.group)
                    .map_err(xlsx_error)?;
            }
            for (offset, field) in group.fields.iter().enumerate() {
                let title = match &field.unit {
                    Some(unit) if !unit.is_empty() => format!("{} ({unit})", field.name),
                    _ => field.name.clone(),
                };
                sheet
                    .write_string_with_format(1, column + offset as u16, title, &style.header)
                    .map_err(xlsx_error)?;
            }
            column = last + 1;
        }
        sheet.set_column_width(2, 50).map_err(xlsx_error)?;
        sheet.set_freeze_panes(2, 4).map_err(xlsx_error)?;
        let columns = aspect_columns(snapshot);
        for (index, item) in snapshot.matrix.rows.iter().enumerate() {
            let row = index as u32 + 2;
            sheet
                .write_number(row, 0, item.sequence as f64)
                .map_err(xlsx_error)?;
            put(sheet, row, 1, &item.reference)?;
            put(sheet, row, 2, &item.title)?;
            if let Some(year) = item.year {
                sheet
                    .write_number(row, 3, year as f64)
                    .map_err(xlsx_error)?;
            }
            for (offset, (_, field)) in columns.iter().enumerate() {
                put(
                    sheet,
                    row,
                    4 + offset as u16,
                    &cell_text(item.cells.get(&field.id).map_or(&[][..], Vec::as_slice)),
                )?;
            }
        }
    }

    let mut sheets = 7;
    if extended {
        write_extended_sheets(&mut workbook, snapshot, &style)?;
        sheets += 5;
    }
    workbook.save(path).map_err(xlsx_error)?;
    Ok(format!(
        "Libro con {sheets} hojas{}",
        if extended {
            " (versión ampliada con procedencia)"
        } else {
            " (formato compatible)"
        }
    ))
}

fn write_extended_sheets(
    workbook: &mut Workbook,
    snapshot: &ProjectSnapshot,
    style: &Styles,
) -> AppResult<()> {
    {
        let sheet = workbook
            .add_worksheet()
            .set_name("Extraccion_procedencia")
            .map_err(xlsx_error)?;
        header(
            sheet,
            0,
            &[
                "Id",
                "Reference",
                "Grupo",
                "Aspecto",
                "Valor",
                "Estado",
                "Cita literal",
                "Página",
                "Registrado por",
                "Actualizado",
                "Notas",
            ],
            &style.header,
        )?;
        sheet.set_column_width(6, 70).map_err(xlsx_error)?;
        let columns = aspect_columns(snapshot);
        let mut row = 1;
        for item in &snapshot.matrix.rows {
            for (group, field) in &columns {
                for value in item.cells.get(&field.id).into_iter().flatten() {
                    let evidence = value.evidence.first();
                    sheet
                        .write_number(row, 0, item.sequence as f64)
                        .map_err(xlsx_error)?;
                    put(sheet, row, 1, &item.reference)?;
                    put(sheet, row, 2, group)?;
                    put(sheet, row, 3, &field.name)?;
                    put(sheet, row, 4, &value.display)?;
                    put(sheet, row, 5, status_text(&value.status))?;
                    put(
                        sheet,
                        row,
                        6,
                        evidence.map_or("", |evidence| evidence.quote.as_str()),
                    )?;
                    if let Some(page) = evidence.and_then(|evidence| evidence.page) {
                        sheet
                            .write_number(row, 7, page as f64)
                            .map_err(xlsx_error)?;
                    }
                    put(sheet, row, 8, &value.actor_name)?;
                    put(sheet, row, 9, &value.updated_at)?;
                    put(sheet, row, 10, &value.notes)?;
                    row += 1;
                }
            }
        }
    }
    {
        let sheet = workbook
            .add_worksheet()
            .set_name("I_E_decisiones")
            .map_err(xlsx_error)?;
        header(
            sheet,
            0,
            &[
                "Id",
                "Title",
                "Fase",
                "Revisor",
                "Decisión",
                "Criterio",
                "Justificación",
                "Final",
                "Fecha",
            ],
            &style.header,
        )?;
        let codes: HashMap<&str, &str> = snapshot
            .screening_metadata
            .criteria
            .iter()
            .map(|criterion| (criterion.id.as_str(), criterion.code.as_str()))
            .collect();
        let mut row = 1;
        for (phase, table) in [
            ("Metadatos", &snapshot.screening_metadata),
            ("Texto completo", &snapshot.screening_full_text),
        ] {
            for item in &table.rows {
                for decision in item.reviews.iter().chain(item.final_decision.iter()) {
                    sheet
                        .write_number(row, 0, item.sequence as f64)
                        .map_err(xlsx_error)?;
                    put(sheet, row, 1, &item.title)?;
                    put(sheet, row, 2, phase)?;
                    put(sheet, row, 3, &decision.reviewer_name)?;
                    put(sheet, row, 4, &decision.decision)?;
                    put(
                        sheet,
                        row,
                        5,
                        decision
                            .criterion_id
                            .as_deref()
                            .and_then(|id| codes.get(id))
                            .copied()
                            .unwrap_or(""),
                    )?;
                    put(sheet, row, 6, &decision.reason)?;
                    put(sheet, row, 7, if decision.is_final { "Sí" } else { "No" })?;
                    put(sheet, row, 8, &decision.decided_at)?;
                    row += 1;
                }
            }
        }
    }
    {
        let sheet = workbook
            .add_worksheet()
            .set_name("Calidad_revisores")
            .map_err(xlsx_error)?;
        header(
            sheet,
            0,
            &[
                "Id",
                "Title",
                "Revisor",
                "Pregunta",
                "Puntaje",
                "Justificación",
            ],
            &style.header,
        )?;
        let mut row = 1;
        for item in &snapshot.quality.rows {
            for assessment in item.reviews.iter().chain(item.final_assessment.iter()) {
                for entry in &assessment.scores {
                    let code = snapshot
                        .quality
                        .criteria
                        .iter()
                        .find(|criterion| criterion.id == entry.criterion_id)
                        .map_or("", |criterion| criterion.code.as_str());
                    sheet
                        .write_number(row, 0, item.sequence as f64)
                        .map_err(xlsx_error)?;
                    put(sheet, row, 1, &item.title)?;
                    put(sheet, row, 2, &assessment.reviewer_name)?;
                    put(sheet, row, 3, code)?;
                    if let Some(score) = entry.score {
                        sheet.write_number(row, 4, score).map_err(xlsx_error)?;
                    }
                    put(sheet, row, 5, &entry.justification)?;
                    row += 1;
                }
            }
        }
    }
    {
        let sheet = workbook
            .add_worksheet()
            .set_name("Auditoria")
            .map_err(xlsx_error)?;
        header(
            sheet,
            0,
            &["Fecha", "Actor", "Entidad", "Acción", "Antes", "Después"],
            &style.header,
        )?;
        for (index, entry) in snapshot.audit.iter().enumerate() {
            let row = index as u32 + 1;
            put(sheet, row, 0, &entry.created_at)?;
            put(sheet, row, 1, &entry.actor_name)?;
            put(sheet, row, 2, &entry.entity_type)?;
            put(sheet, row, 3, &entry.action)?;
            put(
                sheet,
                row,
                4,
                &entry
                    .before
                    .as_ref()
                    .map_or(String::new(), Value::to_string),
            )?;
            put(
                sheet,
                row,
                5,
                &entry.after.as_ref().map_or(String::new(), Value::to_string),
            )?;
        }
    }
    {
        let sheet = workbook
            .add_worksheet()
            .set_name("Exportacion")
            .map_err(xlsx_error)?;
        let protocol = snapshot.workspace.protocol.first();
        let rows = [
            ("Formato", SNAPSHOT_FORMAT.to_owned()),
            ("ResearchPilot", snapshot.app_version.to_owned()),
            ("Generado", snapshot.generated_at.clone()),
            ("Proyecto", snapshot.workspace.project.id.clone()),
            ("Protocolo", protocol.map_or("—".into(), |version| format!("v{} {}", version.version_number, version.status))),
            ("Revisión", if snapshot.workspace.project.screening_mode == "double" { "Doble".into() } else { "Simple".into() }),
            ("Conflictos abiertos", snapshot.workspace.open_conflicts.to_string()),
            ("Estados", "Confirmado = validado por un revisor; Propuesto / Por revisar = sin validar; No reportado = el estudio no lo informa (decisión humana); No encontrado = no localizado".into()),
        ];
        sheet.set_column_width(1, 100).map_err(xlsx_error)?;
        for (index, (label, value)) in rows.iter().enumerate() {
            sheet
                .write_string_with_format(index as u32, 0, *label, &style.header)
                .map_err(xlsx_error)?;
            put(sheet, index as u32, 1, value)?;
        }
    }
    Ok(())
}

// --- CSV ------------------------------------------------------------------------------

fn matrix_csv(snapshot: &ProjectSnapshot) -> AppResult<String> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    let columns = aspect_columns(snapshot);
    let mut titles: Vec<String> = ["Id", "Reference", "Title", "Year"]
        .iter()
        .map(|title| title.to_string())
        .collect();
    titles.extend(
        columns
            .iter()
            .map(|(group, field)| format!("{group} · {}", field.name)),
    );
    writer
        .write_record(&titles)
        .map_err(|error| AppError::Validation(error.to_string()))?;
    for item in &snapshot.matrix.rows {
        let mut record = vec![
            item.sequence.to_string(),
            item.reference.clone(),
            item.title.clone(),
            item.year.map_or(String::new(), |year| year.to_string()),
        ];
        record.extend(
            columns.iter().map(|(_, field)| {
                cell_text(item.cells.get(&field.id).map_or(&[][..], Vec::as_slice))
            }),
        );
        writer
            .write_record(&record)
            .map_err(|error| AppError::Validation(error.to_string()))?;
    }
    let bytes = writer
        .into_inner()
        .map_err(|error| AppError::Validation(error.to_string()))?;
    // UTF-8 BOM so spreadsheet apps detect accents correctly.
    Ok(format!("\u{feff}{}", String::from_utf8_lossy(&bytes)))
}

// --- Bibliographies -----------------------------------------------------------------

fn bibliography_rows<'a>(snapshot: &'a ProjectSnapshot, scope: &str) -> Vec<&'a ScreeningRow> {
    if scope == "corpus" {
        snapshot.screening_metadata.rows.iter().collect()
    } else {
        included_rows(snapshot)
    }
}

fn is_conference(venue: Option<&str>) -> bool {
    venue.is_some_and(|venue| {
        [
            "conference",
            "proceedings",
            "symposium",
            "workshop",
            "congreso",
        ]
        .iter()
        .any(|word| venue.to_lowercase().contains(word))
    })
}

pub fn ris(rows: &[&ScreeningRow]) -> String {
    let mut output = String::new();
    for row in rows {
        let conference = is_conference(row.venue.as_deref());
        let _ = writeln!(output, "TY  - {}", if conference { "CONF" } else { "JOUR" });
        let _ = writeln!(output, "TI  - {}", row.title);
        for author in &row.authors {
            let _ = writeln!(output, "AU  - {author}");
        }
        if let Some(year) = row.year {
            let _ = writeln!(output, "PY  - {year}");
        }
        if let Some(venue) = row.venue.as_deref().filter(|venue| !venue.is_empty()) {
            let _ = writeln!(
                output,
                "{}  - {venue}",
                if conference { "T2" } else { "JO" }
            );
        }
        if let Some(doi) = row.doi.as_deref().filter(|doi| !doi.is_empty()) {
            let _ = writeln!(output, "DO  - {doi}");
        }
        if let Some(text) = row.abstract_text.as_deref().filter(|text| !text.is_empty()) {
            let _ = writeln!(output, "AB  - {}", text.replace('\n', " "));
        }
        for keyword in &row.keywords {
            let _ = writeln!(output, "KW  - {keyword}");
        }
        let _ = writeln!(output, "ER  - \n");
    }
    output
}

fn bibtex_escape(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' | '%' | '#' | '_' | '$' => {
                output.push('\\');
                output.push(character);
            }
            '{' | '}' | '\\' => {}
            '\n' => output.push(' '),
            other => output.push(other),
        }
    }
    output
}

fn surname(author: &str) -> String {
    let author = author.trim();
    let surname = match author.split_once(',') {
        Some((last, _)) => last.to_owned(),
        None => author
            .split_whitespace()
            .last()
            .unwrap_or("anon")
            .to_owned(),
    };
    surname
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_lowercase()
}

pub fn bibtex(rows: &[&ScreeningRow]) -> String {
    let mut output = String::new();
    let mut used: HashMap<String, usize> = HashMap::new();
    for row in rows {
        let first_word: String = row
            .title
            .split_whitespace()
            .map(|word| {
                word.chars()
                    .filter(char::is_ascii_alphanumeric)
                    .collect::<String>()
                    .to_lowercase()
            })
            .find(|word| word.len() > 3)
            .unwrap_or_default();
        let base = format!(
            "{}{}{}",
            row.authors
                .first()
                .map_or("anon".into(), |author| surname(author)),
            row.year.map_or(String::new(), |year| year.to_string()),
            first_word
        );
        let count = used.entry(base.clone()).or_insert(0);
        let key = if *count == 0 {
            base.clone()
        } else {
            format!("{base}{}", (b'a' + (*count as u8 - 1).min(25)) as char)
        };
        *count += 1;
        let conference = is_conference(row.venue.as_deref());
        let _ = writeln!(
            output,
            "@{}{{{key},",
            if conference {
                "inproceedings"
            } else {
                "article"
            }
        );
        let _ = writeln!(output, "  title = {{{}}},", bibtex_escape(&row.title));
        if !row.authors.is_empty() {
            let _ = writeln!(
                output,
                "  author = {{{}}},",
                row.authors
                    .iter()
                    .map(|author| bibtex_escape(author))
                    .collect::<Vec<_>>()
                    .join(" and ")
            );
        }
        if let Some(year) = row.year {
            let _ = writeln!(output, "  year = {{{year}}},");
        }
        if let Some(venue) = row.venue.as_deref().filter(|venue| !venue.is_empty()) {
            let _ = writeln!(
                output,
                "  {} = {{{}}},",
                if conference { "booktitle" } else { "journal" },
                bibtex_escape(venue)
            );
        }
        if let Some(doi) = row.doi.as_deref().filter(|doi| !doi.is_empty()) {
            let _ = writeln!(
                output,
                "  doi = {{{}}},",
                bibtex_escape(doi).replace("\\_", "_")
            );
        }
        if !row.keywords.is_empty() {
            let _ = writeln!(
                output,
                "  keywords = {{{}}},",
                bibtex_escape(&row.keywords.join(", "))
            );
        }
        if let Some(text) = row.abstract_text.as_deref().filter(|text| !text.is_empty()) {
            let _ = writeln!(output, "  abstract = {{{}}},", bibtex_escape(text));
        }
        output.push_str("}\n\n");
    }
    output
}

// --- Markdown and LaTeX reports ---------------------------------------------------------

fn md_cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn markdown(snapshot: &ProjectSnapshot) -> String {
    let project = &snapshot.workspace.project;
    let mut output = String::new();
    let _ = writeln!(output, "# {}\n", project.name);
    let _ = writeln!(
        output,
        "> Generado por ResearchPilot {} el {}. Los valores provienen de decisiones y extracciones registradas; revisa los marcados como propuestos antes de reportarlos.\n",
        snapshot.app_version, snapshot.generated_at
    );
    let counts = &snapshot.workspace.counts;
    let _ = writeln!(
        output,
        "## Resumen\n\n| Corpus | Elegibles | Incluidos | Excluidos |\n|---|---|---|---|\n| {} | {} | {} | {} |\n",
        counts.total, counts.eligible, counts.included, counts.excluded
    );
    if !snapshot.strategies.is_empty() {
        let _ = writeln!(
            output,
            "## Cadenas de búsqueda\n\n| Base de datos | Fecha | Cadena | Cantidad |\n|---|---|---|---|"
        );
        for strategy in &snapshot.strategies {
            let _ = writeln!(
                output,
                "| {} | {} | `{}` | {} |",
                md_cell(&strategy.database_name),
                strategy.query_date,
                md_cell(&strategy.query_text).replace('`', "'"),
                strategy.result_count
            );
        }
        output.push('\n');
    }
    let _ = writeln!(output, "## Estudios incluidos\n");
    for row in included_rows(snapshot) {
        let _ = writeln!(output, "{}. {}", row.sequence, full_reference(row));
    }
    output.push('\n');
    for group in snapshot
        .matrix
        .groups
        .iter()
        .filter(|group| !group.fields.is_empty())
    {
        let _ = writeln!(
            output,
            "## {}{}\n",
            group.label,
            group
                .question
                .as_ref()
                .map_or(String::new(), |question| format!(" — {question}"))
        );
        let _ = writeln!(
            output,
            "| Id | Estudio | {} |",
            group
                .fields
                .iter()
                .map(|field| md_cell(&field.name))
                .collect::<Vec<_>>()
                .join(" | ")
        );
        let _ = writeln!(output, "|---|---|{}", "---|".repeat(group.fields.len()));
        for row in &snapshot.matrix.rows {
            let cells: Vec<String> = group
                .fields
                .iter()
                .map(|field| {
                    md_cell(&cell_text(
                        row.cells.get(&field.id).map_or(&[][..], Vec::as_slice),
                    ))
                })
                .collect();
            let _ = writeln!(
                output,
                "| {} | {} | {} |",
                row.sequence,
                md_cell(&row.reference),
                cells.join(" | ")
            );
        }
        output.push('\n');
    }
    let _ = writeln!(output, "## Anexo de evidencias\n");
    let columns = aspect_columns(snapshot);
    for row in &snapshot.matrix.rows {
        for (group, field) in &columns {
            for value in row
                .cells
                .get(&field.id)
                .into_iter()
                .flatten()
                .filter(|value| value.status != "rejected")
            {
                for evidence in &value.evidence {
                    let _ = writeln!(
                        output,
                        "- **{}** · {} · {}: {} ({}) — «{}»{}",
                        row.reference,
                        group,
                        field.name,
                        if value.display.is_empty() {
                            status_text(&value.status)
                        } else {
                            &value.display
                        },
                        status_text(&value.status),
                        evidence.quote.replace('\n', " "),
                        evidence
                            .page
                            .map_or(String::new(), |page| format!(", p. {page}"))
                    );
                }
            }
        }
    }
    output
}

pub fn latex_escape(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' => output.push_str("\\textbackslash{}"),
            '&' | '%' | '$' | '#' | '_' | '{' | '}' => {
                output.push('\\');
                output.push(character);
            }
            '~' => output.push_str("\\textasciitilde{}"),
            '^' => output.push_str("\\textasciicircum{}"),
            '\n' => output.push(' '),
            other => output.push(other),
        }
    }
    output
}

fn latex(snapshot: &ProjectSnapshot) -> String {
    let project = &snapshot.workspace.project;
    let mut output = String::from(
        "\\documentclass[10pt]{article}\n\\usepackage[utf8]{inputenc}\n\\usepackage[T1]{fontenc}\n\\usepackage[spanish]{babel}\n\\usepackage{longtable}\n\\usepackage{booktabs}\n\\usepackage[margin=2cm,landscape]{geometry}\n\\begin{document}\n",
    );
    let _ = writeln!(output, "\\section*{{{}}}", latex_escape(&project.name));
    let _ = writeln!(
        output,
        "\\noindent Generado por ResearchPilot {} el {}. Estudios incluidos: {}.\n",
        snapshot.app_version,
        latex_escape(&snapshot.generated_at),
        snapshot.workspace.counts.included
    );
    for group in snapshot
        .matrix
        .groups
        .iter()
        .filter(|group| !group.fields.is_empty())
    {
        let _ = writeln!(
            output,
            "\\subsection*{{{}}}",
            latex_escape(&format!(
                "{}{}",
                group.label,
                group
                    .question
                    .as_ref()
                    .map_or(String::new(), |question| format!(" — {question}"))
            ))
        );
        let width = (20.0 / group.fields.len() as f64).clamp(2.5, 8.0);
        let _ = writeln!(
            output,
            "\\begin{{longtable}}{{p{{0.6cm}}p{{3.2cm}}{}}}\n\\toprule",
            format!("p{{{width:.1}cm}}").repeat(group.fields.len())
        );
        let _ = writeln!(
            output,
            "Id & Estudio & {} \\\\\n\\midrule\n\\endhead",
            group
                .fields
                .iter()
                .map(|field| latex_escape(&field.name))
                .collect::<Vec<_>>()
                .join(" & ")
        );
        for row in &snapshot.matrix.rows {
            let cells: Vec<String> = group
                .fields
                .iter()
                .map(|field| {
                    latex_escape(&cell_text(
                        row.cells.get(&field.id).map_or(&[][..], Vec::as_slice),
                    ))
                })
                .collect();
            let _ = writeln!(
                output,
                "{} & {} & {} \\\\",
                row.sequence,
                latex_escape(&row.reference),
                cells.join(" & ")
            );
        }
        output.push_str("\\bottomrule\n\\end{longtable}\n\n");
    }
    output.push_str("\\subsection*{Estudios incluidos}\n\\begin{enumerate}\n");
    for row in included_rows(snapshot) {
        let _ = writeln!(output, "\\item {}", latex_escape(&full_reference(row)));
    }
    output.push_str("\\end{enumerate}\n\\subsection*{Anexo de evidencias}\n\\begin{itemize}\n");
    let columns = aspect_columns(snapshot);
    for row in &snapshot.matrix.rows {
        for (group, field) in &columns {
            for value in row
                .cells
                .get(&field.id)
                .into_iter()
                .flatten()
                .filter(|value| value.status != "rejected")
            {
                for evidence in &value.evidence {
                    let _ = writeln!(
                        output,
                        "\\item \\textbf{{{}}} · {} · {}: {} --- ``{}''{}",
                        latex_escape(&row.reference),
                        latex_escape(group),
                        latex_escape(&field.name),
                        latex_escape(if value.display.is_empty() {
                            status_text(&value.status)
                        } else {
                            &value.display
                        }),
                        latex_escape(&evidence.quote),
                        evidence
                            .page
                            .map_or(String::new(), |page| format!(", p.~{page}"))
                    );
                }
            }
        }
    }
    output.push_str("\\end{itemize}\n\\end{document}\n");
    output
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
