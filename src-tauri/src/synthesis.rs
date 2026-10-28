//! Phase 8: PRISMA 2020 counts derived from recorded events, the flow diagram,
//! descriptive synthesis from verified values and editable report drafts.
//!
//! Nothing here is estimated: every number is counted from imports, corpus
//! membership and final decisions. Pending work is reported as pending and
//! the arithmetic of the flow is checked, so an inconsistent or incomplete
//! diagram is flagged instead of silently "balanced".

use std::{collections::BTreeMap, fmt::Write as _};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    db::{Database, now},
    error::{AppError, AppResult},
    review::{active_reviewer, audit, ensure_project},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceCount {
    pub source: String,
    pub records: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReasonCount {
    pub reason: String,
    pub reports: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrismaCounts {
    pub databases: Vec<SourceCount>,
    pub records_from_databases: i64,
    pub records_from_other_sources: i64,
    pub duplicates_removed: i64,
    pub removed_other_reasons: i64,
    pub records_screened: i64,
    pub records_excluded: i64,
    pub screening_pending: i64,
    pub reports_sought: i64,
    pub reports_not_retrieved: i64,
    pub reports_assessed: i64,
    pub reports_excluded: i64,
    pub exclusion_reasons: Vec<ReasonCount>,
    pub full_text_pending: i64,
    pub studies_included: i64,
    pub secondary_studies: i64,
    /// Arithmetic checks that failed; empty means the flow balances.
    pub issues: Vec<String>,
    /// True when no study is waiting for a decision.
    pub complete: bool,
}

impl Database {
    pub fn prisma_counts(&self, project_id: &str) -> AppResult<PrismaCounts> {
        let connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let count = |sql: &str| -> AppResult<i64> {
            Ok(connection.query_row(sql, [project_id], |row| row.get(0))?)
        };

        let databases: Vec<SourceCount> = {
            let mut statement = connection.prepare(
                "SELECT s.database_name, COUNT(sr.id) FROM search_strategies s
                 JOIN search_imports si ON si.search_strategy_id = s.id
                 JOIN source_records sr ON sr.import_batch_id = si.import_batch_id AND sr.record_status <> 'error'
                 WHERE s.project_id = ?1 AND s.deleted_at IS NULL
                 GROUP BY s.database_name ORDER BY s.database_name",
            )?;
            statement
                .query_map([project_id], |row| {
                    Ok(SourceCount {
                        source: row.get(0)?,
                        records: row.get(1)?,
                    })
                })?
                .collect::<Result<_, _>>()?
        };
        let records_from_databases: i64 = databases.iter().map(|source| source.records).sum();
        // Articles added to the corpus without any project search record (e.g. from the library).
        let from_databases_condition = "EXISTS(SELECT 1 FROM search_imports si JOIN search_strategies s ON s.id = si.search_strategy_id AND s.project_id = r.project_id AND s.deleted_at IS NULL
                                                JOIN source_records sr ON sr.import_batch_id = si.import_batch_id AND sr.article_id = r.article_id)";
        let records_from_other_sources = count(&format!(
            "SELECT COUNT(*) FROM review_articles r WHERE r.project_id = ?1 AND NOT {from_databases_condition}"
        ))?;
        let unique_from_databases = count(&format!(
            "SELECT COUNT(*) FROM review_articles r WHERE r.project_id = ?1 AND {from_databases_condition}"
        ))?;
        let removed_other_reasons = count(
            "SELECT COUNT(*) FROM review_articles WHERE project_id = ?1 AND removed_at IS NOT NULL",
        )?;
        let records_screened = count(
            "SELECT COUNT(*) FROM review_articles WHERE project_id = ?1 AND removed_at IS NULL",
        )?;

        let final_of = |phase: &str| -> String {
            format!(
                "(SELECT d.decision FROM screening_decisions d WHERE d.project_id = r.project_id AND d.article_id = r.article_id
                  AND d.phase = '{phase}' AND d.is_final = 1 ORDER BY d.decided_at DESC, d.rowid DESC LIMIT 1)"
            )
        };
        let metadata = final_of("metadata");
        let full_text = final_of("full_text");
        let active = "FROM review_articles r WHERE r.project_id = ?1 AND r.removed_at IS NULL";
        let records_excluded = count(&format!(
            "SELECT COUNT(*) {active} AND {metadata} = 'exclude'"
        ))?;
        let reports_sought = count(&format!(
            "SELECT COUNT(*) {active} AND {metadata} IN ('include', 'awaiting_full_text')"
        ))?;
        let screening_pending = records_screened - records_excluded - reports_sought;
        let reports_assessed = count(&format!(
            "SELECT COUNT(*) {active} AND {metadata} IN ('include', 'awaiting_full_text') AND {full_text} IN ('include', 'exclude')"
        ))?;
        let reports_not_retrieved = count(&format!(
            "SELECT COUNT(*) {active} AND {metadata} IN ('include', 'awaiting_full_text') AND {full_text} IS NULL
             AND NOT EXISTS(SELECT 1 FROM document_files f WHERE f.article_id = r.article_id AND f.deleted_at IS NULL)"
        ))?;
        let reports_excluded = count(&format!(
            "SELECT COUNT(*) {active} AND {metadata} IN ('include', 'awaiting_full_text') AND {full_text} = 'exclude'"
        ))?;
        let studies_included = count(&format!(
            "SELECT COUNT(*) {active} AND {metadata} IN ('include', 'awaiting_full_text') AND {full_text} = 'include'"
        ))?;
        let full_text_pending = reports_sought - reports_not_retrieved - reports_assessed;
        let secondary_studies = count(
            "SELECT COUNT(*) FROM review_articles WHERE project_id = ?1 AND removed_at IS NULL AND is_secondary = 1",
        )?;

        let exclusion_reasons: Vec<ReasonCount> = {
            let mut statement = connection.prepare(&format!(
                "SELECT COALESCE(c.name, 'Sin criterio registrado'), COUNT(*)
                 FROM review_articles r
                 LEFT JOIN eligibility_criteria c ON c.id = (
                     SELECT d.criterion_id FROM screening_decisions d
                     WHERE d.project_id = r.project_id AND d.article_id = r.article_id AND d.phase = 'full_text' AND d.is_final = 1
                     ORDER BY d.decided_at DESC, d.rowid DESC LIMIT 1)
                 WHERE r.project_id = ?1 AND r.removed_at IS NULL
                   AND {metadata} IN ('include', 'awaiting_full_text') AND {full_text} = 'exclude'
                 GROUP BY 1 ORDER BY 2 DESC, 1"
            ))?;
            statement
                .query_map([project_id], |row| {
                    Ok(ReasonCount {
                        reason: row.get(0)?,
                        reports: row.get(1)?,
                    })
                })?
                .collect::<Result<_, _>>()?
        };

        // Records that matched an article already in the corpus (by DOI or repeated imports).
        let duplicates_removed = (records_from_databases - unique_from_databases).max(0);
        let mut counts = PrismaCounts {
            databases,
            records_from_databases,
            records_from_other_sources,
            duplicates_removed,
            removed_other_reasons,
            records_screened,
            records_excluded,
            screening_pending,
            reports_sought,
            reports_not_retrieved,
            reports_assessed,
            reports_excluded,
            exclusion_reasons,
            full_text_pending,
            studies_included,
            secondary_studies,
            issues: Vec::new(),
            complete: false,
        };
        counts.issues = check_flow(&counts);
        counts.complete = counts.screening_pending == 0 && counts.full_text_pending == 0;
        Ok(counts)
    }
}

/// PRISMA arithmetic: every box must equal the sum of its outflows.
pub fn check_flow(counts: &PrismaCounts) -> Vec<String> {
    let mut issues = Vec::new();
    let identified = counts.records_from_databases + counts.records_from_other_sources;
    if identified - counts.duplicates_removed - counts.removed_other_reasons
        != counts.records_screened
    {
        issues.push(format!(
            "Identificados ({identified}) − duplicados ({}) − eliminados ({}) ≠ cribados ({})",
            counts.duplicates_removed, counts.removed_other_reasons, counts.records_screened
        ));
    }
    if counts.records_excluded + counts.reports_sought + counts.screening_pending
        != counts.records_screened
        || counts.screening_pending < 0
    {
        issues
            .push("Los registros cribados no cuadran con excluidos + buscados + pendientes".into());
    }
    if counts.reports_not_retrieved + counts.reports_assessed + counts.full_text_pending
        != counts.reports_sought
        || counts.full_text_pending < 0
    {
        issues.push(
            "Los informes buscados no cuadran con no recuperados + evaluados + pendientes".into(),
        );
    }
    if counts.reports_excluded + counts.studies_included != counts.reports_assessed {
        issues.push("Los informes evaluados no cuadran con excluidos + incluidos".into());
    }
    if counts
        .exclusion_reasons
        .iter()
        .map(|reason| reason.reports)
        .sum::<i64>()
        != counts.reports_excluded
    {
        issues
            .push("La suma de motivos de exclusión no coincide con los informes excluidos".into());
    }
    issues
}

// --- PRISMA 2020 flow diagram (SVG) --------------------------------------------------

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Wrap text to roughly `width` characters per line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + word.chars().count() + 1 > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

struct Canvas(String);

impl Canvas {
    fn boxed(&mut self, x: f64, y: f64, width: f64, height: f64, lines: &[String]) {
        let _ = writeln!(
            self.0,
            r##"<rect x="{x}" y="{y}" width="{width}" height="{height}" rx="6" fill="#ffffff" stroke="#334155" stroke-width="1.2"/>"##
        );
        for (index, line) in lines.iter().enumerate() {
            let bold = index == 0;
            let _ = writeln!(
                self.0,
                r##"<text x="{}" y="{}" font-size="12" font-family="Helvetica, Arial, sans-serif" fill="#0f172a"{}>{}</text>"##,
                x + 10.0,
                y + 20.0 + index as f64 * 16.0,
                if bold { r#" font-weight="bold""# } else { "" },
                xml(line)
            );
        }
    }

    fn arrow(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let _ = writeln!(
            self.0,
            r##"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="#334155" stroke-width="1.4" marker-end="url(#arrow)"/>"##
        );
    }

    fn band(&mut self, y: f64, height: f64, label: &str) {
        let _ = writeln!(
            self.0,
            r##"<rect x="10" y="{y}" width="34" height="{height}" rx="6" fill="#ccfbf1" stroke="#0d9488"/>"##
        );
        let _ = writeln!(
            self.0,
            r##"<text transform="translate(32,{}) rotate(-90)" text-anchor="middle" font-size="13" font-weight="bold" font-family="Helvetica, Arial, sans-serif" fill="#134e4a">{}</text>"##,
            y + height / 2.0,
            xml(label)
        );
    }
}

fn height_for(lines: &[String]) -> f64 {
    16.0 * lines.len() as f64 + 18.0
}

pub fn prisma_svg(counts: &PrismaCounts, title: &str) -> String {
    let left = 60.0;
    let width = 330.0;
    let right = 450.0;
    let right_width = 330.0;
    let mut canvas = Canvas(String::new());

    let mut identification = vec!["Registros identificados".to_owned()];
    identification.push(format!(
        "Bases de datos (n = {})",
        counts.records_from_databases
    ));
    for source in &counts.databases {
        identification.push(format!("  {} (n = {})", source.source, source.records));
    }
    identification.push(format!(
        "Otras fuentes (n = {})",
        counts.records_from_other_sources
    ));
    let removed = vec![
        "Registros eliminados antes del cribado".to_owned(),
        format!("Duplicados (n = {})", counts.duplicates_removed),
        format!(
            "Eliminados por otros motivos (n = {})",
            counts.removed_other_reasons
        ),
    ];
    let mut y = 50.0;
    let identification_height = height_for(&identification).max(height_for(&removed));
    canvas.band(y, identification_height, "Identificación");
    canvas.boxed(left, y, width, identification_height, &identification);
    canvas.boxed(right, y, right_width, identification_height, &removed);
    canvas.arrow(
        left + width,
        y + identification_height / 2.0,
        right,
        y + identification_height / 2.0,
    );
    let screening_top = y + identification_height + 40.0;
    canvas.arrow(
        left + width / 2.0,
        y + identification_height,
        left + width / 2.0,
        screening_top,
    );

    y = screening_top;
    let screened = vec![
        "Registros cribados".to_owned(),
        format!("(n = {})", counts.records_screened),
        format!("Pendientes de decisión (n = {})", counts.screening_pending),
    ];
    let excluded = vec![
        "Registros excluidos".to_owned(),
        format!("(n = {})", counts.records_excluded),
    ];
    let row_height = height_for(&screened);
    canvas.boxed(left, y, width, row_height, &screened);
    canvas.boxed(right, y, right_width, row_height, &excluded);
    canvas.arrow(
        left + width,
        y + row_height / 2.0,
        right,
        y + row_height / 2.0,
    );
    canvas.arrow(
        left + width / 2.0,
        y + row_height,
        left + width / 2.0,
        y + row_height + 30.0,
    );

    let screening_band_top = y;
    y += row_height + 30.0;
    let sought = vec![
        "Informes buscados para recuperar".to_owned(),
        format!("(n = {})", counts.reports_sought),
    ];
    let not_retrieved = vec![
        "Informes no recuperados".to_owned(),
        format!("(n = {})", counts.reports_not_retrieved),
    ];
    let sought_height = height_for(&sought);
    canvas.boxed(left, y, width, sought_height, &sought);
    canvas.boxed(right, y, right_width, sought_height, &not_retrieved);
    canvas.arrow(
        left + width,
        y + sought_height / 2.0,
        right,
        y + sought_height / 2.0,
    );
    canvas.arrow(
        left + width / 2.0,
        y + sought_height,
        left + width / 2.0,
        y + sought_height + 30.0,
    );

    y += sought_height + 30.0;
    let assessed = vec![
        "Informes evaluados para elegibilidad".to_owned(),
        format!("(n = {})", counts.reports_assessed),
        format!(
            "Pendientes de texto completo (n = {})",
            counts.full_text_pending
        ),
    ];
    let mut reasons = vec![format!(
        "Informes excluidos (n = {})",
        counts.reports_excluded
    )];
    for reason in &counts.exclusion_reasons {
        for (index, line) in wrap(&format!("{} (n = {})", reason.reason, reason.reports), 44)
            .into_iter()
            .enumerate()
        {
            reasons.push(if index == 0 {
                format!("• {line}")
            } else {
                format!("  {line}")
            });
        }
    }
    let assessed_height = height_for(&assessed).max(height_for(&reasons));
    canvas.boxed(left, y, width, assessed_height, &assessed);
    canvas.boxed(right, y, right_width, assessed_height, &reasons);
    canvas.arrow(
        left + width,
        y + assessed_height / 2.0,
        right,
        y + assessed_height / 2.0,
    );
    let screening_band_height = y + assessed_height - screening_band_top;
    canvas.band(screening_band_top, screening_band_height, "Cribado");
    canvas.arrow(
        left + width / 2.0,
        y + assessed_height,
        left + width / 2.0,
        y + assessed_height + 40.0,
    );

    y += assessed_height + 40.0;
    let included = vec![
        "Estudios incluidos en la revisión".to_owned(),
        format!("(n = {})", counts.studies_included),
    ];
    let included_height = height_for(&included);
    canvas.band(y, included_height, "Incluidos");
    canvas.boxed(left, y, width, included_height, &included);
    y += included_height + 24.0;

    let mut footer = String::new();
    if !counts.complete {
        footer.push_str("Diagrama provisional: hay registros pendientes de decisión. ");
    }
    if !counts.issues.is_empty() {
        footer.push_str(
            "Advertencia: la contabilidad no cuadra; revisa los datos antes de reportar.",
        );
    }
    let mut footer_lines = 0;
    for line in wrap(&footer, 110) {
        let _ = writeln!(
            canvas.0,
            r##"<text x="{left}" y="{}" font-size="11" font-family="Helvetica, Arial, sans-serif" fill="#b45309">{}</text>"##,
            y + footer_lines as f64 * 14.0,
            xml(&line)
        );
        footer_lines += 1;
    }
    let total_height = y + footer_lines as f64 * 14.0 + 16.0;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 {total_height}" width="800" height="{total_height}" role="img" aria-label="Diagrama de flujo PRISMA 2020">
<title>{}</title>
<defs><marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#334155"/></marker></defs>
<rect width="800" height="{total_height}" fill="#ffffff"/>
<text x="{left}" y="30" font-size="15" font-weight="bold" font-family="Helvetica, Arial, sans-serif" fill="#0f172a">{}</text>
{}</svg>
"##,
        xml(&format!("PRISMA 2020 — {title}")),
        xml(&format!("Diagrama de flujo PRISMA 2020 — {title}")),
        canvas.0
    )
}

// --- Descriptive synthesis ------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frequency {
    pub value: String,
    pub count: i64,
    /// Project Ids of the studies counted, for traceability.
    pub studies: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumericGroup {
    pub unit: String,
    pub n: i64,
    pub min: f64,
    pub max: f64,
    pub median: f64,
    pub values: Vec<(i64, f64)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldSynthesis {
    pub field_id: String,
    pub group: String,
    pub name: String,
    pub data_type: String,
    /// Included studies with at least one confirmed value.
    pub studies_with_data: i64,
    /// Values left out because they are not confirmed (proposed / needs review).
    pub unverified_values: i64,
    pub frequencies: Vec<Frequency>,
    pub numeric: Vec<NumericGroup>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Synthesis {
    pub included: i64,
    pub by_year: Vec<Frequency>,
    pub by_venue: Vec<Frequency>,
    pub by_source: Vec<Frequency>,
    pub fields: Vec<FieldSynthesis>,
}

fn add(map: &mut BTreeMap<String, Vec<i64>>, key: String, study: i64) {
    let studies = map.entry(key).or_default();
    if !studies.contains(&study) {
        studies.push(study);
    }
}

fn frequencies(map: BTreeMap<String, Vec<i64>>) -> Vec<Frequency> {
    let mut items: Vec<Frequency> = map
        .into_iter()
        .map(|(value, studies)| Frequency {
            count: studies.len() as i64,
            value,
            studies,
        })
        .collect();
    items.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.value.cmp(&right.value))
    });
    items
}

impl Database {
    pub fn synthesis(&self, project_id: &str) -> AppResult<Synthesis> {
        let matrix = self.extraction_matrix(project_id, false)?;
        let screening = self.screening_table(project_id, "metadata")?;
        let included: Vec<_> = screening
            .rows
            .iter()
            .filter(|row| row.stage == "included")
            .collect();
        let mut years = BTreeMap::new();
        let mut venues = BTreeMap::new();
        let mut sources = BTreeMap::new();
        for row in &included {
            add(
                &mut years,
                row.year.map_or("Sin año".into(), |year| year.to_string()),
                row.sequence,
            );
            add(
                &mut venues,
                row.venue
                    .clone()
                    .filter(|venue| !venue.is_empty())
                    .unwrap_or_else(|| "Sin venue".into()),
                row.sequence,
            );
            add(
                &mut sources,
                row.source_database
                    .clone()
                    .unwrap_or_else(|| "Otras fuentes".into()),
                row.sequence,
            );
        }
        let mut by_year = frequencies(years);
        by_year.sort_by(|left, right| left.value.cmp(&right.value));

        let mut fields = Vec::new();
        for group in &matrix.groups {
            for field in &group.fields {
                let mut categories = BTreeMap::new();
                let mut numbers: BTreeMap<String, Vec<(i64, f64)>> = BTreeMap::new();
                let mut with_data = 0;
                let mut unverified = 0;
                for row in &matrix.rows {
                    let values = row.cells.get(&field.id).map_or(&[][..], Vec::as_slice);
                    unverified += values
                        .iter()
                        .filter(|value| {
                            matches!(value.status.as_str(), "proposed" | "needs_review")
                        })
                        .count() as i64;
                    let confirmed: Vec<_> = values
                        .iter()
                        .filter(|value| value.status == "confirmed")
                        .collect();
                    if !confirmed.is_empty() {
                        with_data += 1;
                    }
                    for value in confirmed {
                        match &value.value {
                            Value::Array(items) => {
                                items.iter().filter_map(Value::as_str).for_each(|item| {
                                    add(&mut categories, item.to_owned(), row.sequence)
                                })
                            }
                            Value::Bool(flag) => add(
                                &mut categories,
                                if *flag { "Sí".into() } else { "No".into() },
                                row.sequence,
                            ),
                            Value::Number(number) if field.data_type == "number" => {
                                numbers
                                    .entry(field.unit.clone().unwrap_or_default())
                                    .or_default()
                                    .push((row.sequence, number.as_f64().unwrap_or_default()));
                            }
                            Value::String(text)
                                if matches!(field.data_type.as_str(), "category" | "date") =>
                            {
                                add(&mut categories, text.clone(), row.sequence)
                            }
                            _ => {}
                        }
                    }
                }
                let numeric: Vec<NumericGroup> = numbers
                    .into_iter()
                    .map(|(unit, mut values)| {
                        let mut sorted: Vec<f64> = values.iter().map(|(_, value)| *value).collect();
                        sorted.sort_by(f64::total_cmp);
                        let middle = sorted.len() / 2;
                        let median = if sorted.len().is_multiple_of(2) {
                            (sorted[middle - 1] + sorted[middle]) / 2.0
                        } else {
                            sorted[middle]
                        };
                        values.sort_by_key(|(study, _)| *study);
                        NumericGroup {
                            unit,
                            n: sorted.len() as i64,
                            min: sorted[0],
                            max: sorted[sorted.len() - 1],
                            median,
                            values,
                        }
                    })
                    .collect();
                let mut warnings = Vec::new();
                if numeric.len() > 1 {
                    warnings.push("Hay valores con unidades distintas: se resumen por separado y no deben compararse directamente.".into());
                }
                if field.data_type == "number" && !numeric.is_empty() {
                    warnings.push("Resumen descriptivo: verifica que dataset, configuración y métrica sean comparables antes de interpretar diferencias.".into());
                }
                if unverified > 0 {
                    warnings.push(format!(
                        "{unverified} valor(es) sin confirmar no se incluyen."
                    ));
                }
                fields.push(FieldSynthesis {
                    field_id: field.id.clone(),
                    group: group.label.clone(),
                    name: field.name.clone(),
                    data_type: field.data_type.clone(),
                    studies_with_data: with_data,
                    unverified_values: unverified,
                    frequencies: frequencies(categories),
                    numeric,
                    warnings,
                });
            }
        }
        Ok(Synthesis {
            included: included.len() as i64,
            by_year,
            by_venue: frequencies(venues),
            by_source: frequencies(sources),
            fields,
        })
    }
}

// --- Report drafts ----------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub section: String,
    pub content: String,
    pub generated_content: String,
    pub edited: bool,
    pub generated_at: String,
    pub updated_at: String,
}

fn study_refs(studies: &[i64]) -> String {
    let ids: Vec<String> = studies.iter().map(i64::to_string).collect();
    format!("[Id {}]", ids.join(", "))
}

impl Database {
    /// Deterministic draft built only from recorded, verified data. Every claim
    /// carries the project Ids of the studies that support it.
    pub fn generate_draft_text(&self, project_id: &str, section: &str) -> AppResult<String> {
        let workspace = self.project_workspace(project_id)?;
        let counts = self.prisma_counts(project_id)?;
        let mut text = String::new();
        match section {
            "methods" => {
                let protocol = workspace
                    .protocol
                    .iter()
                    .find(|version| version.status == "approved");
                let _ = writeln!(text, "## Metodología (borrador)\n");
                match protocol {
                    Some(version) => {
                        let _ = writeln!(
                            text,
                            "La revisión siguió el protocolo aprobado (versión {}). ",
                            version.version_number
                        );
                    }
                    None => {
                        let _ = writeln!(text, "[Pendiente: el protocolo aún no está aprobado.] ");
                    }
                }
                let strategies = self.search_strategies(project_id)?;
                if !strategies.is_empty() {
                    let _ = writeln!(
                        text,
                        "Se consultaron {} fuente(s): {}.",
                        strategies
                            .iter()
                            .map(|strategy| &strategy.database_name)
                            .collect::<std::collections::BTreeSet<_>>()
                            .len(),
                        strategies
                            .iter()
                            .map(|strategy| format!(
                                "{} ({}, {} resultados)",
                                strategy.database_name, strategy.query_date, strategy.result_count
                            ))
                            .collect::<Vec<_>>()
                            .join("; ")
                    );
                }
                let criteria = self.eligibility_criteria(project_id)?;
                let inclusion: Vec<_> = criteria
                    .iter()
                    .filter(|criterion| criterion.active && criterion.criterion_type == "inclusion")
                    .map(|criterion| criterion.name.as_str())
                    .collect();
                let exclusion: Vec<_> = criteria
                    .iter()
                    .filter(|criterion| criterion.active && criterion.criterion_type == "exclusion")
                    .map(|criterion| criterion.name.as_str())
                    .collect();
                if !inclusion.is_empty() || !exclusion.is_empty() {
                    let _ = writeln!(
                        text,
                        "\nCriterios de inclusión: {}. Criterios de exclusión: {}.",
                        if inclusion.is_empty() {
                            "ninguno".into()
                        } else {
                            inclusion.join("; ")
                        },
                        if exclusion.is_empty() {
                            "ninguno".into()
                        } else {
                            exclusion.join("; ")
                        }
                    );
                }
                let _ = writeln!(
                    text,
                    "\nEl cribado se realizó en dos fases (título, resumen y palabras clave; luego texto completo) con {}.",
                    if workspace.project.screening_mode == "double" {
                        "dos revisores independientes y resolución de desacuerdos por un tercero"
                    } else {
                        "un revisor"
                    }
                );
                let quality = self.quality_table(project_id)?;
                if !quality.criteria.is_empty() {
                    let _ = writeln!(
                        text,
                        "La calidad se evaluó con {} pregunta(s) ({}) sobre un total máximo de {:.2}.",
                        quality.criteria.len(),
                        quality
                            .criteria
                            .iter()
                            .map(|criterion| criterion.code.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                        quality.max_total
                    );
                }
                let questions = self.research_questions(project_id)?;
                if !questions.questions.is_empty() {
                    let _ = writeln!(text, "\nPreguntas de investigación:");
                    for question in &questions.questions {
                        let _ = writeln!(text, "- {}: {}", question.code, question.question);
                    }
                }
            }
            "results" => {
                let synthesis = self.synthesis(project_id)?;
                let _ = writeln!(text, "## Resultados (borrador)\n");
                let _ = writeln!(
                    text,
                    "Se identificaron {} registros en bases de datos y {} por otras fuentes; tras eliminar {} duplicados se cribaron {} registros, se evaluaron {} informes a texto completo y se incluyeron {} estudios.{}",
                    counts.records_from_databases,
                    counts.records_from_other_sources,
                    counts.duplicates_removed,
                    counts.records_screened,
                    counts.reports_assessed,
                    counts.studies_included,
                    if counts.complete {
                        ""
                    } else {
                        " [Provisional: quedan registros pendientes de decisión.]"
                    }
                );
                if !counts.exclusion_reasons.is_empty() {
                    let _ = writeln!(
                        text,
                        "Motivos de exclusión a texto completo: {}.",
                        counts
                            .exclusion_reasons
                            .iter()
                            .map(|reason| format!("{} (n = {})", reason.reason, reason.reports))
                            .collect::<Vec<_>>()
                            .join("; ")
                    );
                }
                if !synthesis.by_year.is_empty() {
                    let _ = writeln!(
                        text,
                        "\nDistribución por año: {}.",
                        synthesis
                            .by_year
                            .iter()
                            .map(|item| format!(
                                "{} (n = {}) {}",
                                item.value,
                                item.count,
                                study_refs(&item.studies)
                            ))
                            .collect::<Vec<_>>()
                            .join("; ")
                    );
                }
                let mut current_group = String::new();
                for field in &synthesis.fields {
                    if field.frequencies.is_empty() && field.numeric.is_empty() {
                        continue;
                    }
                    if field.group != current_group {
                        current_group = field.group.clone();
                        let _ = writeln!(text, "\n### {current_group}\n");
                    }
                    if !field.frequencies.is_empty() {
                        let _ = writeln!(
                            text,
                            "{}: {}.",
                            field.name,
                            field
                                .frequencies
                                .iter()
                                .map(|item| format!(
                                    "{} en {} estudio(s) {}",
                                    item.value,
                                    item.count,
                                    study_refs(&item.studies)
                                ))
                                .collect::<Vec<_>>()
                                .join("; ")
                        );
                    }
                    for group in &field.numeric {
                        let _ = writeln!(
                            text,
                            "{} ({}): n = {}, rango {}–{}, mediana {} {}.",
                            field.name,
                            if group.unit.is_empty() {
                                "sin unidad"
                            } else {
                                &group.unit
                            },
                            group.n,
                            group.min,
                            group.max,
                            group.median,
                            study_refs(
                                &group
                                    .values
                                    .iter()
                                    .map(|(study, _)| *study)
                                    .collect::<Vec<_>>()
                            )
                        );
                    }
                    if field.unverified_values > 0 {
                        let _ = writeln!(
                            text,
                            "[{} valor(es) sin confirmar en «{}» no se consideraron.]",
                            field.unverified_values, field.name
                        );
                    }
                }
            }
            "limitations" => {
                let _ = writeln!(text, "## Limitaciones (borrador)\n");
                let mut items = Vec::new();
                if !counts.complete {
                    items.push(format!("Quedan {} registro(s) sin decisión de cribado y {} informe(s) pendientes de texto completo.", counts.screening_pending, counts.full_text_pending));
                }
                if counts.reports_not_retrieved > 0 {
                    items.push(format!(
                        "{} informe(s) no pudieron recuperarse a texto completo.",
                        counts.reports_not_retrieved
                    ));
                }
                if workspace.project.screening_mode == "single" {
                    items.push(
                        "El cribado y la evaluación de calidad los realizó un único revisor."
                            .into(),
                    );
                }
                if workspace.open_conflicts > 0 {
                    items.push(format!(
                        "Hay {} conflicto(s) entre revisores sin resolver.",
                        workspace.open_conflicts
                    ));
                }
                let coverage = self.control_coverage(project_id)?;
                if coverage.total > 0 && coverage.found < coverage.total {
                    items.push(format!(
                        "Las cadenas recuperaron {} de {} artículos de control.",
                        coverage.found, coverage.total
                    ));
                }
                let synthesis = self.synthesis(project_id)?;
                let unverified: i64 = synthesis
                    .fields
                    .iter()
                    .map(|field| field.unverified_values)
                    .sum();
                if unverified > 0 {
                    items.push(format!("{unverified} valor(es) extraídos siguen sin confirmar y se excluyeron de la síntesis."));
                }
                let languages: std::collections::BTreeSet<String> = self
                    .search_strategies(project_id)?
                    .into_iter()
                    .filter_map(|strategy| strategy.language)
                    .collect();
                if !languages.is_empty() {
                    items.push(format!(
                        "La búsqueda se restringió a: {}.",
                        languages.into_iter().collect::<Vec<_>>().join(", ")
                    ));
                }
                if items.is_empty() {
                    items.push("[No se detectaron limitaciones en los datos registrados; añade las limitaciones metodológicas propias del estudio.]".into());
                }
                for item in items {
                    let _ = writeln!(text, "- {item}");
                }
            }
            _ => {
                return Err(AppError::Validation(
                    "Sección de borrador no reconocida".into(),
                ));
            }
        }
        let _ = writeln!(
            text,
            "\n> Borrador generado por ResearchPilot a partir de datos registrados el {}. Revísalo antes de usarlo; los números entre corchetes son Id de estudios.",
            now()
        );
        Ok(text)
    }

    pub fn drafts(&self, project_id: &str) -> AppResult<Vec<Draft>> {
        let connection = self.open()?;
        ensure_project(&connection, project_id)?;
        let mut statement = connection.prepare(
            "SELECT section, content, generated_content, content <> generated_content, generated_at, updated_at
             FROM synthesis_drafts WHERE project_id = ?1 ORDER BY CASE section WHEN 'methods' THEN 0 WHEN 'results' THEN 1 ELSE 2 END",
        )?;
        Ok(statement
            .query_map([project_id], |row| {
                Ok(Draft {
                    section: row.get(0)?,
                    content: row.get(1)?,
                    generated_content: row.get(2)?,
                    edited: row.get(3)?,
                    generated_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }

    /// (Re)generate a section. The previous text is kept in the audit trail.
    pub fn generate_draft(&self, project_id: &str, section: &str) -> AppResult<Vec<Draft>> {
        let text = self.generate_draft_text(project_id, section)?;
        let connection = self.open()?;
        let actor = active_reviewer(&connection, project_id)?;
        let before: Option<String> = connection
            .query_row(
                "SELECT content FROM synthesis_drafts WHERE project_id = ?1 AND section = ?2",
                params![project_id, section],
                |row| row.get(0),
            )
            .optional()?;
        let timestamp = now();
        connection.execute(
            "INSERT INTO synthesis_drafts (project_id, section, content, generated_content, generated_at, updated_at) VALUES (?1, ?2, ?3, ?3, ?4, ?4)
             ON CONFLICT(project_id, section) DO UPDATE SET content = excluded.content, generated_content = excluded.generated_content,
                generated_at = excluded.generated_at, updated_at = excluded.updated_at",
            params![project_id, section, text, timestamp],
        )?;
        audit(
            &connection,
            project_id,
            "synthesis_draft",
            section,
            "generated",
            &actor,
            before.map(|content| json!({ "content": content })),
            json!({ "content": text }),
        )?;
        self.drafts(project_id)
    }

    pub fn save_draft(
        &self,
        project_id: &str,
        section: &str,
        content: &str,
    ) -> AppResult<Vec<Draft>> {
        let connection = self.open()?;
        let actor = active_reviewer(&connection, project_id)?;
        let before: String = connection
            .query_row(
                "SELECT content FROM synthesis_drafts WHERE project_id = ?1 AND section = ?2",
                params![project_id, section],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                AppError::Validation("Genera primero el borrador de esta sección".into())
            })?;
        connection.execute(
            "UPDATE synthesis_drafts SET content = ?1, updated_at = ?2 WHERE project_id = ?3 AND section = ?4",
            params![content, now(), project_id, section],
        )?;
        audit(
            &connection,
            project_id,
            "synthesis_draft",
            section,
            "edited",
            &actor,
            Some(json!({ "content": before })),
            json!({ "content": content }),
        )?;
        self.drafts(project_id)
    }
}

#[cfg(test)]
#[path = "synthesis_tests.rs"]
mod tests;
