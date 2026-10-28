//! Master record ("ficha maestra") persistence and human review.
//!
//! Extractor output is stored as proposals with literal evidence. Only people
//! confirm values or declare them `not_reported`; every change creates an
//! immutable revision and an audit event, and any revision can be restored.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    db::{Database, now},
    error::{AppError, AppResult},
    jobs::ExtractionContext,
    models::{
        AddMasterValueInput, ArticleExtraction, EvidenceItem, EvidenceListItem,
        ExtractionRunSummary, MasterField, MasterValue, PageSummary, ReferenceEntry, SectionEntry,
        TableFigure, UpdateMasterValueInput, ValueRevision,
    },
};

const HUMAN_STATUSES: [&str; 5] = [
    "confirmed",
    "needs_review",
    "not_reported",
    "not_applicable",
    "rejected",
];
const MAX_QUOTE_CHARS: usize = 1200;

fn json_text(value: &impl serde::Serialize) -> AppResult<String> {
    serde_json::to_string(value).map_err(|error| AppError::Worker(error.to_string()))
}

fn bbox_from(value: &Value) -> Option<Vec<f64>> {
    let items = value.as_array()?;
    (items.len() == 4)
        .then(|| items.iter().filter_map(Value::as_f64).collect::<Vec<_>>())
        .filter(|bbox| bbox.len() == 4)
}

fn truncate(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

impl Database {
    /// Store one worker result atomically and return a short summary.
    pub(crate) fn persist_extraction(
        &self,
        run_id: &str,
        article_id: &str,
        context: &ExtractionContext,
        payload: &Value,
    ) -> AppResult<String> {
        if payload["documentSha256"].as_str() != Some(context.sha256.as_str()) {
            return Err(AppError::Worker(
                "La huella del documento procesado no coincide con la registrada".into(),
            ));
        }
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        let timestamp = now();

        // Pages: never replace existing text with an empty result.
        let mut page_ids: HashMap<i64, (String, Option<f64>, Option<f64>)> = HashMap::new();
        for page in payload["pages"].as_array().into_iter().flatten() {
            let Some(number) = page["pageNumber"].as_i64().filter(|number| *number > 0) else {
                continue;
            };
            let text = page["text"].as_str().unwrap_or("");
            let status = match page["status"].as_str() {
                Some("extracted") => "extracted",
                Some("failed") => "failed",
                _ => "ocr_required",
            };
            transaction.execute(
                "INSERT INTO document_pages (
                    id, document_file_id, page_number, text_content, width, height, extraction_status,
                    text_source, ocr_confidence, column_count, extraction_run_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(document_file_id, page_number) DO UPDATE SET
                    text_content = CASE WHEN excluded.text_content <> '' THEN excluded.text_content ELSE document_pages.text_content END,
                    extraction_status = CASE WHEN excluded.text_content <> '' THEN excluded.extraction_status
                                             WHEN document_pages.text_content <> '' THEN 'extracted'
                                             ELSE excluded.extraction_status END,
                    text_source = CASE WHEN excluded.text_content <> '' THEN excluded.text_source ELSE document_pages.text_source END,
                    width = COALESCE(excluded.width, document_pages.width),
                    height = COALESCE(excluded.height, document_pages.height),
                    ocr_confidence = excluded.ocr_confidence,
                    column_count = excluded.column_count,
                    extraction_run_id = excluded.extraction_run_id",
                params![
                    Uuid::new_v4().to_string(),
                    context.document_file_id,
                    number,
                    text,
                    page["width"].as_f64(),
                    page["height"].as_f64(),
                    status,
                    page["textSource"].as_str().unwrap_or("none"),
                    page["ocrConfidence"].as_f64(),
                    page["columns"].as_i64(),
                    run_id,
                ],
            )?;
        }
        {
            let mut statement = transaction.prepare("SELECT page_number, id, width, height FROM document_pages WHERE document_file_id = ?1")?;
            let rows = statement.query_map([&context.document_file_id], |row| {
                Ok((row.get(0)?, (row.get(1)?, row.get(2)?, row.get(3)?)))
            })?;
            for row in rows {
                let (number, entry) = row?;
                page_ids.insert(number, entry);
            }
        }

        // Blocks, tables/figures and references are replaced by the newest run.
        transaction.execute(
            "DELETE FROM document_blocks WHERE document_page_id IN (SELECT id FROM document_pages WHERE document_file_id = ?1)",
            [&context.document_file_id],
        )?;
        {
            let mut insert = transaction.prepare(
                "INSERT INTO document_blocks (
                    id, document_page_id, block_type, reading_order, text_content, bounding_box_json,
                    section_path, extraction_run_id, font_size, canonical_section
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for block in payload["blocks"].as_array().into_iter().flatten() {
                let Some((page_id, _, _)) = block["pageNumber"]
                    .as_i64()
                    .and_then(|number| page_ids.get(&number))
                else {
                    continue;
                };
                insert.execute(params![
                    Uuid::new_v4().to_string(),
                    page_id,
                    block["blockType"].as_str().unwrap_or("paragraph"),
                    block["readingOrder"].as_i64().unwrap_or(0),
                    block["text"].as_str().unwrap_or(""),
                    bbox_from(&block["bbox"]).map(|bbox| json!(bbox).to_string()),
                    block["sectionPath"].as_str(),
                    run_id,
                    block["fontSize"].as_f64(),
                    block["canonicalSection"].as_str(),
                ])?;
            }
        }
        transaction.execute(
            "DELETE FROM tables_figures WHERE article_id = ?1 AND extraction_run_id IS NOT NULL",
            [article_id],
        )?;
        for item in payload["tablesFigures"].as_array().into_iter().flatten() {
            let page_number = item["pageNumber"].as_i64();
            let object_type = if item["objectType"].as_str() == Some("figure") {
                "figure"
            } else {
                "table"
            };
            transaction.execute(
                "INSERT INTO tables_figures (
                    id, article_id, document_page_id, object_type, label, caption, content_json,
                    bounding_box_json, extraction_run_id, page_number, caption_bbox_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    Uuid::new_v4().to_string(),
                    article_id,
                    page_number
                        .and_then(|number| page_ids.get(&number))
                        .map(|entry| entry.0.clone()),
                    object_type,
                    item["label"].as_str(),
                    item["caption"].as_str(),
                    item["content"].to_string(),
                    bbox_from(&item["bbox"]).map(|bbox| json!(bbox).to_string()),
                    run_id,
                    page_number,
                    bbox_from(&item["captionBbox"]).map(|bbox| json!(bbox).to_string()),
                ],
            )?;
        }
        transaction.execute(
            "DELETE FROM bibliographic_references WHERE citing_article_id = ?1 AND extraction_run_id IS NOT NULL",
            [article_id],
        )?;
        for reference in payload["references"].as_array().into_iter().flatten() {
            let Some(raw) = reference["raw"]
                .as_str()
                .filter(|raw| !raw.trim().is_empty())
            else {
                continue;
            };
            let doi = reference["doi"].as_str().map(str::to_lowercase);
            transaction.execute(
                "INSERT INTO bibliographic_references (
                    id, citing_article_id, ordinal, raw_reference, title, doi, matched_article_id,
                    extraction_run_id, page_number
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                    (SELECT id FROM articles WHERE lower(doi) = ?6 AND id <> ?2 AND deleted_at IS NULL LIMIT 1),
                    ?7, ?8)",
                params![
                    Uuid::new_v4().to_string(),
                    article_id,
                    reference["ordinal"].as_i64(),
                    truncate(raw, 4000),
                    reference["title"].as_str(),
                    doi,
                    run_id,
                    reference["page"].as_i64(),
                ],
            )?;
        }

        // Earlier machine proposals that nobody touched are superseded by this run.
        transaction.execute(
            "UPDATE master_record_values SET superseded_by_run_id = ?1, updated_at = ?2
             WHERE article_id = ?3 AND superseded_by_run_id IS NULL AND origin <> 'human'
               AND revision_number = 1 AND status IN ('proposed', 'needs_review', 'not_found')",
            params![run_id, timestamp, article_id],
        )?;

        let catalog = field_catalog(&transaction)?;
        let mut existing: HashSet<(String, String)> = HashSet::new();
        {
            let mut statement = transaction.prepare(
                "SELECT field_key, value_json FROM master_record_values WHERE article_id = ?1 AND superseded_by_run_id IS NULL",
            )?;
            for row in statement.query_map([article_id], |row| Ok((row.get(0)?, row.get(1)?)))? {
                existing.insert(row?);
            }
        }
        let engine = payload["engine"].as_str().unwrap_or("unknown");
        let model = format!(
            "{engine}{}",
            if payload["grobidUsed"].as_bool() == Some(true) {
                "+grobid"
            } else {
                ""
            }
        );
        let mut inserted = 0;
        let mut skipped_unknown = 0;
        for candidate in payload["candidates"].as_array().into_iter().flatten() {
            let Some(field_key) = candidate["fieldKey"].as_str() else {
                continue;
            };
            if !catalog.contains_key(field_key) {
                skipped_unknown += 1;
                continue;
            }
            let value_json = candidate["value"].to_string();
            if value_json == "null" || !existing.insert((field_key.to_owned(), value_json.clone()))
            {
                continue;
            }
            let origin = match candidate["origin"].as_str() {
                Some("ocr") => "ocr",
                Some("grobid") => "grobid",
                _ => "rules",
            };
            let status = if candidate["needsReview"].as_bool() == Some(true) || origin == "ocr" {
                "needs_review"
            } else {
                "proposed"
            };
            let evidence = candidate["evidence"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let first = evidence.first().cloned().unwrap_or(Value::Null);
            let value_id = Uuid::new_v4().to_string();
            transaction.execute(
                "INSERT INTO master_record_values (
                    id, article_id, field_key, value_json, display_value, value_type, unit, raw_quote, section,
                    page, bbox_json, document_file_id, source_document_hash, extraction_run_id, provider, model,
                    prompt_version, method, origin, status, notes, revision_number, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'researchpilot-extractor',
                           ?15, ?16, ?17, ?18, ?19, ?20, 1, ?21, ?21)",
                params![
                    value_id,
                    article_id,
                    field_key,
                    value_json,
                    truncate(candidate["displayValue"].as_str().unwrap_or(""), 4000),
                    candidate["valueType"].as_str().unwrap_or("text"),
                    candidate["unit"].as_str(),
                    first["quote"].as_str().map(|quote| truncate(quote, MAX_QUOTE_CHARS)),
                    first["section"].as_str(),
                    first["page"].as_i64().filter(|page| *page > 0),
                    bbox_from(&first["bbox"]).map(|bbox| json!(bbox).to_string()),
                    context.document_file_id,
                    context.sha256,
                    run_id,
                    model,
                    payload["rulesetVersion"].as_str(),
                    candidate["method"].as_str(),
                    origin,
                    status,
                    candidate["notes"].as_str().unwrap_or(""),
                    timestamp,
                ],
            )?;
            insert_revision(
                &transaction,
                &value_id,
                1,
                &value_json,
                candidate["displayValue"].as_str().unwrap_or(""),
                status,
                "",
                "system",
                &format!(
                    "researchpilot-extractor {}",
                    payload["rulesetVersion"].as_str().unwrap_or("")
                ),
                "Propuesta inicial del extractor",
            )?;
            for item in &evidence {
                let Some(quote) = item["quote"]
                    .as_str()
                    .filter(|quote| !quote.trim().is_empty())
                else {
                    continue;
                };
                let page = item["page"].as_i64().filter(|page| *page > 0);
                let page_size = page.and_then(|page| page_ids.get(&page));
                let evidence_id = Uuid::new_v4().to_string();
                transaction.execute(
                    "INSERT INTO evidence_spans (
                        id, article_id, page_start, page_end, quoted_text, source_path, source_sha256,
                        locator_json, created_at, document_file_id, section
                     ) VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        evidence_id,
                        article_id,
                        page,
                        truncate(quote, MAX_QUOTE_CHARS),
                        context.path,
                        context.sha256,
                        json!({
                            "bbox": bbox_from(&item["bbox"]),
                            "pageWidth": page_size.and_then(|entry| entry.1),
                            "pageHeight": page_size.and_then(|entry| entry.2),
                            "method": candidate["method"],
                            "runId": run_id,
                            "verified": true,
                        })
                        .to_string(),
                        timestamp,
                        context.document_file_id,
                        item["section"].as_str(),
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO master_value_evidence (value_id, evidence_id, relation_type) VALUES (?1, ?2, 'supports')",
                    params![value_id, evidence_id],
                )?;
            }
            inserted += 1;
        }

        // Fields the rules searched without a match: "not found by the extractor".
        let mut not_found = 0;
        for field_key in payload["notFound"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !catalog.contains_key(field_key) || existing.iter().any(|(key, _)| key == field_key)
            {
                continue;
            }
            let value_id = Uuid::new_v4().to_string();
            transaction.execute(
                "INSERT INTO master_record_values (
                    id, article_id, field_key, value_json, display_value, value_type, document_file_id,
                    source_document_hash, extraction_run_id, provider, model, prompt_version, method, origin,
                    status, notes, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, 'null', '', ?4, ?5, ?6, ?7, 'researchpilot-extractor', ?8, ?9,
                           'rules:searched', 'rules', 'not_found',
                           'Las reglas buscaron este dato sin encontrarlo. No implica que el artículo no lo reporte.', ?10, ?10)",
                params![
                    value_id,
                    article_id,
                    field_key,
                    catalog[field_key].0,
                    context.document_file_id,
                    context.sha256,
                    run_id,
                    model,
                    payload["rulesetVersion"].as_str(),
                    timestamp,
                ],
            )?;
            insert_revision(
                &transaction,
                &value_id,
                1,
                "null",
                "",
                "not_found",
                "",
                "system",
                "researchpilot-extractor",
                "Búsqueda sin resultados",
            )?;
            not_found += 1;
        }

        // Single-valued fields with competing values need a human decision.
        transaction.execute(
            "UPDATE master_record_values SET status = 'needs_review', updated_at = ?1
             WHERE article_id = ?2 AND superseded_by_run_id IS NULL AND status = 'proposed'
               AND field_key IN (
                 SELECT v.field_key FROM master_record_values v
                 JOIN master_record_fields f ON f.field_key = v.field_key AND f.multi_valued = 0
                 WHERE v.article_id = ?2 AND v.superseded_by_run_id IS NULL
                   AND v.status IN ('proposed', 'needs_review', 'confirmed')
                 GROUP BY v.field_key HAVING COUNT(DISTINCT v.value_json) > 1)",
            params![timestamp, article_id],
        )?;

        transaction.execute(
            "UPDATE extraction_runs SET status = 'completed', completed_at = ?1, engine = ?2, model = ?3,
                    stats_json = ?4, warnings_json = ?5
             WHERE id = ?6",
            params![timestamp, engine, model, payload["stats"].to_string(), payload["warnings"].to_string(), run_id],
        )?;
        let warnings = payload["warnings"].as_array().map_or(0, Vec::len);
        let summary = format!(
            "{inserted} propuesta(s) con evidencia, {not_found} campo(s) no encontrados, {} referencia(s), {warnings} aviso(s)",
            payload["references"].as_array().map_or(0, Vec::len)
        );
        transaction.execute(
            "INSERT INTO audit_events (id, entity_type, entity_id, action, actor_type, actor_name, after_json, created_at)
             VALUES (?1, 'article', ?2, 'scientific_extraction_completed', 'system', 'researchpilot-extractor', ?3, ?4)",
            params![
                Uuid::new_v4().to_string(),
                article_id,
                json!({ "runId": run_id, "proposals": inserted, "notFound": not_found, "unknownFields": skipped_unknown, "engine": model }).to_string(),
                timestamp,
            ],
        )?;
        transaction.commit()?;
        Ok(summary)
    }

    pub fn article_extraction(&self, article_id: &str) -> AppResult<ArticleExtraction> {
        let connection = self.open()?;
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM articles WHERE id = ?1)",
            [article_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(AppError::Validation("El artículo no existe".into()));
        }
        let fields = {
            let mut statement = connection.prepare(
                "SELECT field_key, group_key, label_es, label_en, data_type, multi_valued
                 FROM master_record_fields ORDER BY display_order",
            )?;
            statement
                .query_map([], |row| {
                    Ok(MasterField {
                        field_key: row.get(0)?,
                        group_key: row.get(1)?,
                        label_es: row.get(2)?,
                        label_en: row.get(3)?,
                        data_type: row.get(4)?,
                        multi_valued: row.get(5)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let values = master_values(
            &connection,
            "v.article_id = ?1 AND v.superseded_by_run_id IS NULL",
            article_id,
        )?;
        let runs = {
            let mut statement = connection.prepare(
                "SELECT id, status, provider, model, prompt_version, extractor_version, engine, source_document_hash,
                        started_at, completed_at, error_message, stats_json, warnings_json
                 FROM extraction_runs WHERE article_id = ?1 ORDER BY started_at DESC LIMIT 10",
            )?;
            statement
                .query_map([article_id], |row| {
                    Ok(ExtractionRunSummary {
                        id: row.get(0)?,
                        status: row.get(1)?,
                        provider: row.get(2)?,
                        model: row.get(3)?,
                        prompt_version: row.get(4)?,
                        extractor_version: row.get(5)?,
                        engine: row.get(6)?,
                        source_document_hash: row.get(7)?,
                        started_at: row.get(8)?,
                        completed_at: row.get(9)?,
                        error_message: row.get(10)?,
                        stats: serde_json::from_str(&row.get::<_, String>(11)?)
                            .unwrap_or(Value::Null),
                        warnings: serde_json::from_str(&row.get::<_, String>(12)?)
                            .unwrap_or(Value::Null),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let tables_figures = {
            let mut statement = connection.prepare(
                "SELECT id, object_type, label, caption, page_number, bounding_box_json, caption_bbox_json, content_json
                 FROM tables_figures WHERE article_id = ?1 ORDER BY page_number, label",
            )?;
            statement
                .query_map([article_id], |row| {
                    let bbox: Option<String> = row.get(5)?;
                    let caption_bbox: Option<String> = row.get(6)?;
                    Ok(TableFigure {
                        id: row.get(0)?,
                        object_type: row.get(1)?,
                        label: row.get(2)?,
                        caption: row.get(3)?,
                        page_number: row.get(4)?,
                        bbox: bbox
                            .or(caption_bbox)
                            .and_then(|text| serde_json::from_str(&text).ok()),
                        content: serde_json::from_str(&row.get::<_, String>(7)?)
                            .unwrap_or(Value::Null),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let references = {
            let mut statement = connection.prepare(
                "SELECT id, ordinal, raw_reference, doi, page_number, matched_article_id
                 FROM bibliographic_references WHERE citing_article_id = ?1 ORDER BY ordinal, rowid LIMIT 3000",
            )?;
            statement
                .query_map([article_id], |row| {
                    Ok(ReferenceEntry {
                        id: row.get(0)?,
                        ordinal: row.get(1)?,
                        raw_reference: row.get(2)?,
                        doi: row.get(3)?,
                        page_number: row.get(4)?,
                        matched_article_id: row.get(5)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let sections = {
            let mut statement = connection.prepare(
                "SELECT b.text_content, b.canonical_section, p.page_number, b.bounding_box_json
                 FROM document_blocks b JOIN document_pages p ON p.id = b.document_page_id
                 JOIN document_files d ON d.id = p.document_file_id
                 WHERE d.article_id = ?1 AND d.is_primary = 1 AND d.deleted_at IS NULL AND b.block_type = 'heading'
                 ORDER BY p.page_number, b.reading_order LIMIT 500",
            )?;
            statement
                .query_map([article_id], |row| {
                    let bbox: Option<String> = row.get(3)?;
                    Ok(SectionEntry {
                        title: row.get(0)?,
                        canonical: row.get(1)?,
                        page_number: row.get(2)?,
                        bbox: bbox.and_then(|text| serde_json::from_str(&text).ok()),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let pages = connection.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(p.extraction_status = 'extracted'), 0),
                    COALESCE(SUM(p.text_source = 'ocr'), 0),
                    COALESCE(SUM(p.extraction_status <> 'extracted'), 0)
             FROM document_pages p JOIN document_files d ON d.id = p.document_file_id
             WHERE d.article_id = ?1 AND d.is_primary = 1 AND d.deleted_at IS NULL",
            [article_id],
            |row| {
                Ok(PageSummary {
                    total: row.get(0)?,
                    extracted: row.get(1)?,
                    ocr: row.get(2)?,
                    without_text: row.get(3)?,
                })
            },
        )?;
        Ok(ArticleExtraction {
            fields,
            values,
            runs,
            active_job: self.active_extraction_job(&connection, article_id)?,
            tables_figures,
            references,
            sections,
            pages,
        })
    }

    pub fn update_master_value(&self, input: UpdateMasterValueInput) -> AppResult<MasterValue> {
        if !HUMAN_STATUSES.contains(&input.status.as_str()) {
            return Err(AppError::Validation("Estado de revisión no válido".into()));
        }
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        let (article_id, field_key, value_json, display, status, notes, revision, superseded): (String, String, String, String, String, String, i64, Option<String>) = transaction
            .query_row(
                "SELECT article_id, field_key, value_json, display_value, status, notes, revision_number, superseded_by_run_id
                 FROM master_record_values WHERE id = ?1",
                [&input.value_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::Validation("El valor no existe".into()))?;
        if superseded.is_some() {
            return Err(AppError::Validation(
                "Este valor pertenece a una extracción anterior".into(),
            ));
        }
        let data_type = field_catalog(&transaction)?
            .remove(&field_key)
            .map(|entry| entry.0)
            .ok_or_else(|| AppError::Validation("Campo desconocido".into()))?;
        let (new_json, new_display) = match &input.value_text {
            Some(text) => parse_value(&data_type, text)?,
            None => (value_json.clone(), display.clone()),
        };
        validate_status_value(&input.status, &new_json)?;
        let notes_text = input.notes.trim();
        if new_json == value_json && input.status == status && notes_text == notes {
            return master_value_by_id(&transaction, &input.value_id);
        }
        let reviewer = local_reviewer(&transaction, self)?;
        let timestamp = now();
        transaction.execute(
            "UPDATE master_record_values SET value_json = ?1, display_value = ?2, status = ?3, notes = ?4,
                    reviewer_id = ?5, reviewed_at = ?6, revision_number = ?7, updated_at = ?6
             WHERE id = ?8",
            params![new_json, new_display, input.status, notes_text, reviewer.0, timestamp, revision + 1, input.value_id],
        )?;
        insert_revision(
            &transaction,
            &input.value_id,
            revision + 1,
            &new_json,
            &new_display,
            &input.status,
            notes_text,
            "human",
            &reviewer.1,
            input.reason.trim(),
        )?;
        audit_value(
            &transaction,
            &article_id,
            &input.value_id,
            "master_value_reviewed",
            &reviewer.1,
            json!({ "value": value_json, "status": status, "notes": notes }),
            json!({ "value": new_json, "status": input.status, "notes": notes_text, "reason": input.reason.trim() }),
        )?;
        let value = master_value_by_id(&transaction, &input.value_id)?;
        transaction.commit()?;
        Ok(value)
    }

    pub fn add_master_value(&self, input: AddMasterValueInput) -> AppResult<MasterValue> {
        if !HUMAN_STATUSES.contains(&input.status.as_str()) || input.status == "rejected" {
            return Err(AppError::Validation(
                "Estado no válido para un valor nuevo".into(),
            ));
        }
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        let data_type = field_catalog(&transaction)?
            .remove(&input.field_key)
            .map(|entry| entry.0)
            .ok_or_else(|| AppError::Validation("Campo desconocido".into()))?;
        let (value_json, display) = match input
            .value_text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            Some(text) => parse_value(&data_type, text)?,
            None => ("null".into(), String::new()),
        };
        validate_status_value(&input.status, &value_json)?;
        let document: Option<(String, String, String)> = transaction
            .query_row(
                "SELECT d.id, d.sha256, d.original_path FROM articles a
                 LEFT JOIN document_files d ON d.article_id = a.id AND d.is_primary = 1 AND d.deleted_at IS NULL
                 WHERE a.id = ?1 AND a.deleted_at IS NULL",
                [&input.article_id],
                |row| Ok((row.get::<_, Option<String>>(0)?.unwrap_or_default(), row.get::<_, Option<String>>(1)?.unwrap_or_default(), row.get::<_, Option<String>>(2)?.unwrap_or_default())),
            )
            .optional()?;
        let Some((document_id, sha256, path)) = document else {
            return Err(AppError::Validation("El artículo no existe".into()));
        };
        if input.page.is_some_and(|page| page < 1) {
            return Err(AppError::Validation(
                "La página debe ser mayor que cero".into(),
            ));
        }
        let quote = input
            .quote
            .as_deref()
            .map(str::trim)
            .filter(|quote| !quote.is_empty());
        let reviewer = local_reviewer(&transaction, self)?;
        let timestamp = now();
        let value_id = Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO master_record_values (
                id, article_id, field_key, value_json, display_value, value_type, raw_quote, page,
                document_file_id, source_document_hash, method, origin, status, reviewer_id, reviewed_at,
                notes, revision_number, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULLIF(?9, ''), NULLIF(?10, ''), 'manual', 'human', ?11, ?12, ?13, ?14, 1, ?13, ?13)",
            params![value_id, input.article_id, input.field_key, value_json, display, data_type, quote.map(|quote| truncate(quote, MAX_QUOTE_CHARS)), input.page, document_id, sha256, input.status, reviewer.0, timestamp, input.notes.trim()],
        )?;
        insert_revision(
            &transaction,
            &value_id,
            1,
            &value_json,
            &display,
            &input.status,
            input.notes.trim(),
            "human",
            &reviewer.1,
            "Valor registrado manualmente",
        )?;
        if let Some(quote) = quote {
            // A manual quote is checked against the stored page text; mismatches stay visible.
            let verified = match input.page {
                Some(page) if !document_id.is_empty() => transaction
                    .query_row(
                        "SELECT instr(lower(text_content), lower(?1)) > 0 FROM document_pages WHERE document_file_id = ?2 AND page_number = ?3",
                        params![quote, document_id, page],
                        |row| row.get::<_, bool>(0),
                    )
                    .optional()?
                    .unwrap_or(false),
                _ => false,
            };
            let evidence_id = Uuid::new_v4().to_string();
            transaction.execute(
                "INSERT INTO evidence_spans (id, article_id, page_start, page_end, quoted_text, source_path, source_sha256,
                                             locator_json, created_at, document_file_id)
                 VALUES (?1, ?2, ?3, ?3, ?4, NULLIF(?5, ''), NULLIF(?6, ''), ?7, ?8, NULLIF(?9, ''))",
                params![evidence_id, input.article_id, input.page, truncate(quote, MAX_QUOTE_CHARS), path, sha256, json!({ "method": "manual", "verified": verified }).to_string(), timestamp, document_id],
            )?;
            transaction.execute(
                "INSERT INTO master_value_evidence (value_id, evidence_id, relation_type) VALUES (?1, ?2, 'supports')",
                params![value_id, evidence_id],
            )?;
        }
        audit_value(
            &transaction,
            &input.article_id,
            &value_id,
            "master_value_added",
            &reviewer.1,
            Value::Null,
            json!({ "field": input.field_key, "value": value_json, "status": input.status }),
        )?;
        let value = master_value_by_id(&transaction, &value_id)?;
        transaction.commit()?;
        Ok(value)
    }

    pub fn master_value_history(&self, value_id: &str) -> AppResult<Vec<ValueRevision>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT revision_number, value_json, display_value, status, notes, actor_type, actor_name, reason, created_at
             FROM master_record_value_revisions WHERE value_id = ?1 ORDER BY revision_number DESC",
        )?;
        Ok(statement
            .query_map([value_id], |row| {
                Ok(ValueRevision {
                    revision_number: row.get(0)?,
                    value: serde_json::from_str(&row.get::<_, String>(1)?).unwrap_or(Value::Null),
                    display_value: row.get(2)?,
                    status: row.get(3)?,
                    notes: row.get(4)?,
                    actor_type: row.get(5)?,
                    actor_name: row.get(6)?,
                    reason: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    /// Restore an earlier revision by creating a new one with its content.
    pub fn revert_master_value(
        &self,
        value_id: &str,
        revision_number: i64,
    ) -> AppResult<MasterValue> {
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        let (value_json, display, status, notes): (String, String, String, String) = transaction
            .query_row(
                "SELECT value_json, display_value, status, notes FROM master_record_value_revisions
                 WHERE value_id = ?1 AND revision_number = ?2",
                params![value_id, revision_number],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::Validation("La revisión no existe".into()))?;
        let (article_id, current_revision, before_json, before_status): (String, i64, String, String) = transaction.query_row(
            "SELECT article_id, revision_number, value_json, status FROM master_record_values WHERE id = ?1 AND superseded_by_run_id IS NULL",
            [value_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        let reviewer = local_reviewer(&transaction, self)?;
        let timestamp = now();
        transaction.execute(
            "UPDATE master_record_values SET value_json = ?1, display_value = ?2, status = ?3, notes = ?4,
                    reviewer_id = ?5, reviewed_at = ?6, revision_number = ?7, updated_at = ?6
             WHERE id = ?8",
            params![value_json, display, status, notes, reviewer.0, timestamp, current_revision + 1, value_id],
        )?;
        let reason = format!("Revertido a la revisión {revision_number}");
        insert_revision(
            &transaction,
            value_id,
            current_revision + 1,
            &value_json,
            &display,
            &status,
            &notes,
            "human",
            &reviewer.1,
            &reason,
        )?;
        audit_value(
            &transaction,
            &article_id,
            value_id,
            "master_value_reverted",
            &reviewer.1,
            json!({ "value": before_json, "status": before_status }),
            json!({ "value": value_json, "status": status, "reason": reason }),
        )?;
        let value = master_value_by_id(&transaction, value_id)?;
        transaction.commit()?;
        Ok(value)
    }

    pub fn recent_evidence(&self, limit: i64) -> AppResult<Vec<EvidenceListItem>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT e.id, a.id, a.title, v.id, v.field_key, f.label_es, v.status, v.display_value, e.quoted_text,
                    e.page_start, e.created_at
             FROM evidence_spans e
             JOIN master_value_evidence link ON link.evidence_id = e.id
             JOIN master_record_values v ON v.id = link.value_id AND v.superseded_by_run_id IS NULL
             JOIN master_record_fields f ON f.field_key = v.field_key
             JOIN articles a ON a.id = e.article_id AND a.deleted_at IS NULL
             ORDER BY CASE v.status WHEN 'needs_review' THEN 0 WHEN 'proposed' THEN 1 ELSE 2 END, e.created_at DESC
             LIMIT ?1",
        )?;
        Ok(statement
            .query_map([limit.clamp(1, 1000)], |row| {
                Ok(EvidenceListItem {
                    id: row.get(0)?,
                    article_id: row.get(1)?,
                    article_title: row.get(2)?,
                    value_id: row.get(3)?,
                    field_key: row.get(4)?,
                    field_label: row.get(5)?,
                    value_status: row.get(6)?,
                    display_value: row.get(7)?,
                    quote: row.get(8)?,
                    page: row.get(9)?,
                    created_at: row.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }
}

/// `field_key -> (data_type, multi_valued)`
fn field_catalog(connection: &Connection) -> AppResult<HashMap<String, (String, bool)>> {
    let mut statement = connection
        .prepare("SELECT field_key, data_type, multi_valued FROM master_record_fields")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?)))
    })?;
    Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
}

fn parse_value(data_type: &str, text: &str) -> AppResult<(String, String)> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(("null".into(), String::new()));
    }
    let value = match data_type {
        "number" => {
            let number: f64 = text
                .replace(',', "")
                .parse()
                .map_err(|_| AppError::Validation("El valor debe ser numérico".into()))?;
            json!(number)
        }
        "list" => {
            let items: Vec<String> = text
                .split(';')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect();
            return Ok((json_text(&items)?, items.join("; ")));
        }
        "url" if !(text.starts_with("http://") || text.starts_with("https://")) => {
            return Err(AppError::Validation(
                "La URL debe comenzar con http:// o https://".into(),
            ));
        }
        _ => json!(text),
    };
    Ok((value.to_string(), text.to_owned()))
}

fn validate_status_value(status: &str, value_json: &str) -> AppResult<()> {
    if status == "confirmed" && value_json == "null" {
        return Err(AppError::Validation(
            "No se puede confirmar un valor vacío; usa «No reportado» o «No aplica»".into(),
        ));
    }
    Ok(())
}

fn local_reviewer(
    transaction: &Transaction<'_>,
    database: &Database,
) -> AppResult<(String, String)> {
    let name = database.settings()?.researcher_name.trim().to_owned();
    let name = if name.is_empty() {
        "Investigador local".to_owned()
    } else {
        name
    };
    let existing: Option<String> = transaction
        .query_row(
            "SELECT id FROM reviewers WHERE is_local_profile = 1 LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let id = match existing {
        Some(id) => {
            transaction.execute(
                "UPDATE reviewers SET display_name = ?1 WHERE id = ?2",
                params![name, id],
            )?;
            id
        }
        None => {
            let id = Uuid::new_v4().to_string();
            transaction.execute(
                "INSERT INTO reviewers (id, display_name, is_local_profile, created_at) VALUES (?1, ?2, 1, ?3)",
                params![id, name, now()],
            )?;
            id
        }
    };
    Ok((id, name))
}

#[allow(clippy::too_many_arguments)]
fn insert_revision(
    transaction: &Transaction<'_>,
    value_id: &str,
    revision: i64,
    value_json: &str,
    display: &str,
    status: &str,
    notes: &str,
    actor_type: &str,
    actor_name: &str,
    reason: &str,
) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO master_record_value_revisions (
            id, value_id, revision_number, value_json, display_value, status, notes, actor_type, actor_name, reason, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![Uuid::new_v4().to_string(), value_id, revision, value_json, display, status, notes, actor_type, actor_name, reason, now()],
    )?;
    Ok(())
}

fn audit_value(
    transaction: &Transaction<'_>,
    article_id: &str,
    value_id: &str,
    action: &str,
    actor: &str,
    before: Value,
    after: Value,
) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO audit_events (id, entity_type, entity_id, action, actor_type, actor_name, before_json, after_json, created_at)
         VALUES (?1, 'master_record_value', ?2, ?3, 'human', ?4, ?5, ?6, ?7)",
        params![
            Uuid::new_v4().to_string(),
            value_id,
            action,
            actor,
            (!before.is_null()).then(|| before.to_string()),
            json!({ "articleId": article_id, "change": after }).to_string(),
            now(),
        ],
    )?;
    Ok(())
}

fn master_value_by_id(connection: &Connection, value_id: &str) -> AppResult<MasterValue> {
    master_values(connection, "v.id = ?1", value_id)?
        .pop()
        .ok_or_else(|| AppError::Validation("El valor no existe".into()))
}

fn master_values(
    connection: &Connection,
    predicate: &str,
    parameter: &str,
) -> AppResult<Vec<MasterValue>> {
    let mut statement = connection.prepare(&format!(
        "SELECT v.id, v.field_key, v.value_json, v.display_value, v.value_type, v.unit, v.origin, v.method, v.status,
                v.notes, v.revision_number, r.display_name, v.reviewed_at, v.extraction_run_id, v.provider, v.model,
                v.prompt_version, v.created_at, v.updated_at, v.article_id,
                (f.multi_valued = 0 AND v.status IN ('proposed', 'needs_review', 'confirmed') AND (
                    SELECT COUNT(DISTINCT other.value_json) FROM master_record_values other
                    WHERE other.article_id = v.article_id AND other.field_key = v.field_key
                      AND other.superseded_by_run_id IS NULL
                      AND other.status IN ('proposed', 'needs_review', 'confirmed')) > 1)
         FROM master_record_values v
         JOIN master_record_fields f ON f.field_key = v.field_key
         LEFT JOIN reviewers r ON r.id = v.reviewer_id
         WHERE {predicate}
         ORDER BY f.display_order, v.created_at"
    ))?;
    let mut values = statement
        .query_map([parameter], |row| {
            Ok(MasterValue {
                id: row.get(0)?,
                field_key: row.get(1)?,
                value: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or(Value::Null),
                display_value: row.get(3)?,
                value_type: row.get(4)?,
                unit: row.get(5)?,
                origin: row.get(6)?,
                method: row.get(7)?,
                status: row.get(8)?,
                notes: row.get(9)?,
                revision_number: row.get(10)?,
                reviewer_name: row.get(11)?,
                reviewed_at: row.get(12)?,
                extraction_run_id: row.get(13)?,
                provider: row.get(14)?,
                model: row.get(15)?,
                prompt_version: row.get(16)?,
                created_at: row.get(17)?,
                updated_at: row.get(18)?,
                conflict: row.get(20)?,
                evidence: Vec::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut evidence_statement = connection.prepare(
        "SELECT e.id, e.quoted_text, e.page_start, e.locator_json, e.section, link.relation_type,
                p.width, p.height
         FROM master_value_evidence link
         JOIN evidence_spans e ON e.id = link.evidence_id
         LEFT JOIN document_pages p ON p.document_file_id = e.document_file_id AND p.page_number = e.page_start
         WHERE link.value_id = ?1 ORDER BY e.page_start, e.created_at",
    )?;
    for value in &mut values {
        value.evidence = evidence_statement
            .query_map([&value.id], |row| {
                let locator: Value =
                    serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or(Value::Null);
                Ok(EvidenceItem {
                    id: row.get(0)?,
                    quote: row.get(1)?,
                    page: row.get(2)?,
                    bbox: bbox_from(&locator["bbox"]),
                    page_width: row
                        .get::<_, Option<f64>>(6)?
                        .or_else(|| locator["pageWidth"].as_f64()),
                    page_height: row
                        .get::<_, Option<f64>>(7)?
                        .or_else(|| locator["pageHeight"].as_f64()),
                    section: row.get(4)?,
                    relation: row.get(5)?,
                    verified: locator["verified"].as_bool(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
    }
    Ok(values)
}
