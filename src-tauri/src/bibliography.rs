use std::{collections::BTreeMap, fs, path::Path};

use csv::StringRecord;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    db::{Database, now},
    error::{AppError, AppResult},
    library::sha256_file,
    models::ImportItemResult,
};

const MAX_REFERENCE_FILES: usize = 20;
const MAX_REFERENCE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_RECORDS_PER_FILE: usize = 10_000;

#[derive(Debug, Clone, Default, Serialize)]
struct ReferenceRecord {
    source_key: Option<String>,
    title: String,
    authors: Vec<String>,
    year: Option<i64>,
    doi: Option<String>,
    abstract_text: Option<String>,
    keywords: Vec<String>,
    venue: Option<String>,
}

impl Database {
    pub fn import_reference_files(&self, paths: Vec<String>) -> AppResult<Vec<ImportItemResult>> {
        if paths.len() > MAX_REFERENCE_FILES {
            return Err(AppError::Validation(format!(
                "Se permiten como máximo {MAX_REFERENCE_FILES} archivos bibliográficos por lote"
            )));
        }
        let mut results = Vec::new();
        for path in paths {
            results.extend(self.import_reference_file(&path)?);
        }
        Ok(results)
    }

    fn import_reference_file(&self, source_path: &str) -> AppResult<Vec<ImportItemResult>> {
        let path = fs::canonicalize(source_path)?;
        validate_reference_file(&path)?;
        let format = reference_format(&path)?;
        let hash = sha256_file(&path)?;
        let connection = self.open()?;
        let previous_batch: Option<String> = connection
            .query_row(
                "SELECT id FROM import_batches
                 WHERE source_sha256 = ?1 AND format = ?2 AND status LIKE 'completed%'
                 ORDER BY created_at DESC LIMIT 1",
                params![hash, format],
                |row| row.get(0),
            )
            .optional()?;
        if previous_batch.is_some() {
            return Ok(vec![ImportItemResult {
                source_path: source_path.into(),
                status: "duplicate".into(),
                article_id: None,
                title: None,
                message: Some("Este archivo bibliográfico ya fue importado sin cambios".into()),
            }]);
        }

        let batch_id = Uuid::new_v4().to_string();
        connection.execute(
            "INSERT INTO import_batches
             (id, source_path, source_sha256, format, status, created_at)
             VALUES (?1, ?2, ?3, ?4, 'running', ?5)",
            params![batch_id, path.to_string_lossy(), hash, format, now()],
        )?;

        let records = match parse_reference_file(&path, format) {
            Ok(records) => records,
            Err(error) => {
                connection.execute(
                    "UPDATE import_batches
                     SET status = 'failed', error_count = 1, completed_at = ?1 WHERE id = ?2",
                    params![now(), batch_id],
                )?;
                return Ok(vec![ImportItemResult {
                    source_path: source_path.into(),
                    status: "error".into(),
                    article_id: None,
                    title: None,
                    message: Some(error.to_string()),
                }]);
            }
        };

        if records.len() > MAX_RECORDS_PER_FILE {
            return Err(AppError::Validation(format!(
                "El archivo supera el límite de {MAX_RECORDS_PER_FILE} registros"
            )));
        }

        let mut imported = 0_i64;
        let mut duplicates = 0_i64;
        let mut errors = 0_i64;
        let mut results = Vec::with_capacity(records.len());
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;

        for record in records {
            let raw_json = serde_json::to_string(&record)
                .map_err(|error| AppError::Initialization(error.to_string()))?;
            if record.title.trim().is_empty() {
                errors += 1;
                transaction.execute(
                    "INSERT INTO source_records
                     (id, import_batch_id, source_key, raw_json, record_status, created_at)
                     VALUES (?1, ?2, ?3, ?4, 'error', ?5)",
                    params![
                        Uuid::new_v4().to_string(),
                        batch_id,
                        record.source_key,
                        raw_json,
                        now()
                    ],
                )?;
                results.push(ImportItemResult {
                    source_path: source_path.into(),
                    status: "error".into(),
                    article_id: None,
                    title: None,
                    message: Some("Registro bibliográfico sin título".into()),
                });
                continue;
            }

            let normalized_doi = record.doi.as_deref().and_then(normalize_doi);
            let duplicate_id = if let Some(doi) = &normalized_doi {
                transaction
                    .query_row(
                        "SELECT id FROM articles WHERE lower(doi) = ?1 AND deleted_at IS NULL LIMIT 1",
                        [doi],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
            } else {
                None
            };

            if let Some(article_id) = duplicate_id {
                duplicates += 1;
                transaction.execute(
                    "INSERT INTO source_records
                     (id, import_batch_id, article_id, source_key, raw_json, record_status, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'duplicate', ?6)",
                    params![Uuid::new_v4().to_string(), batch_id, article_id, record.source_key, raw_json, now()],
                )?;
                results.push(ImportItemResult {
                    source_path: source_path.into(),
                    status: "duplicate".into(),
                    article_id: Some(article_id),
                    title: Some(record.title),
                    message: Some("DOI ya presente; se conservó el registro de procedencia".into()),
                });
                continue;
            }

            let article_id = Uuid::new_v4().to_string();
            let timestamp = now();
            transaction.execute(
                "INSERT INTO articles (
                    id, title, abstract_text, publication_year, doi, authors_json,
                    keywords_json, venue, metadata_source, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
                params![
                    article_id,
                    record.title.trim(),
                    clean_optional(record.abstract_text.clone()),
                    record.year,
                    normalized_doi,
                    serde_json::to_string(&clean_list(record.authors.clone()))
                        .map_err(|error| AppError::Initialization(error.to_string()))?,
                    serde_json::to_string(&clean_list(record.keywords.clone()))
                        .map_err(|error| AppError::Initialization(error.to_string()))?,
                    clean_optional(record.venue.clone()),
                    format,
                    timestamp,
                ],
            )?;
            transaction.execute(
                "INSERT INTO source_records
                 (id, import_batch_id, article_id, source_key, raw_json, record_status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'imported', ?6)",
                params![
                    Uuid::new_v4().to_string(),
                    batch_id,
                    article_id,
                    record.source_key,
                    raw_json,
                    now()
                ],
            )?;
            transaction.execute(
                "INSERT INTO audit_events
                 (id, entity_type, entity_id, action, actor_type, actor_name, after_json, created_at)
                 VALUES (?1, 'article', ?2, 'metadata_imported', 'import', ?3, ?4, ?5)",
                params![Uuid::new_v4().to_string(), article_id, format, raw_json, now()],
            )?;
            register_title_candidate(&transaction, &article_id, &record)?;
            imported += 1;
            results.push(ImportItemResult {
                source_path: source_path.into(),
                status: "imported".into(),
                article_id: Some(article_id),
                title: Some(record.title),
                message: None,
            });
        }

        let status = if errors > 0 {
            "completed_with_errors"
        } else {
            "completed"
        };
        transaction.execute(
            "UPDATE import_batches SET status = ?1, imported_count = ?2,
                duplicate_count = ?3, error_count = ?4, completed_at = ?5 WHERE id = ?6",
            params![status, imported, duplicates, errors, now(), batch_id],
        )?;
        transaction.commit()?;
        Ok(results)
    }
}

fn register_title_candidate(
    connection: &Connection,
    article_id: &str,
    record: &ReferenceRecord,
) -> AppResult<()> {
    let candidate: Option<String> = connection
        .query_row(
            "SELECT id FROM articles
             WHERE id <> ?1 AND deleted_at IS NULL
               AND lower(trim(title)) = lower(trim(?2))
               AND (publication_year = ?3 OR (?3 IS NULL AND publication_year IS NULL))
             ORDER BY created_at LIMIT 1",
            params![article_id, record.title, record.year],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(candidate_id) = candidate {
        connection.execute(
            "INSERT OR IGNORE INTO duplicate_candidates
             (id, article_id, candidate_article_id, reasons_json, similarity_score, created_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            params![
                Uuid::new_v4().to_string(),
                article_id,
                candidate_id,
                serde_json::json!(["exact_title", "same_year"]).to_string(),
                now()
            ],
        )?;
    }
    Ok(())
}

fn validate_reference_file(path: &Path) -> AppResult<()> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_REFERENCE_BYTES {
        return Err(AppError::Validation(format!(
            "El archivo bibliográfico debe tener entre 1 byte y {} MB",
            MAX_REFERENCE_BYTES / 1024 / 1024
        )));
    }
    Ok(())
}

fn reference_format(path: &Path) -> AppResult<&'static str> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("ris") => Ok("ris"),
        Some("bib") | Some("bibtex") => Ok("bibtex"),
        Some("csv") => Ok("csv"),
        _ => Err(AppError::Validation(
            "Formato no compatible; usa RIS, BibTeX o CSV".into(),
        )),
    }
}

fn parse_reference_file(path: &Path, format: &str) -> AppResult<Vec<ReferenceRecord>> {
    match format {
        "ris" => parse_ris(&fs::read_to_string(path)?),
        "bibtex" => parse_bibtex(&fs::read_to_string(path)?),
        "csv" => parse_csv(path),
        _ => Err(AppError::Validation(
            "Formato bibliográfico no reconocido".into(),
        )),
    }
}

fn parse_ris(content: &str) -> AppResult<Vec<ReferenceRecord>> {
    let mut records = Vec::new();
    let mut fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in content.lines() {
        let Some((tag, value)) = line.split_once("  - ") else {
            continue;
        };
        if tag == "TY" && !fields.is_empty() {
            records.push(record_from_ris(&fields));
            fields.clear();
        }
        if tag == "ER" {
            records.push(record_from_ris(&fields));
            fields.clear();
        } else {
            fields
                .entry(tag.trim().into())
                .or_default()
                .push(value.trim().into());
        }
    }
    if !fields.is_empty() {
        records.push(record_from_ris(&fields));
    }
    if records.is_empty() {
        return Err(AppError::Validation(
            "El archivo RIS no contiene registros reconocibles".into(),
        ));
    }
    Ok(records)
}

fn record_from_ris(fields: &BTreeMap<String, Vec<String>>) -> ReferenceRecord {
    ReferenceRecord {
        source_key: first(fields, &["ID"]),
        title: first(fields, &["TI", "T1"]).unwrap_or_default(),
        authors: all(fields, &["AU", "A1"]),
        year: first(fields, &["PY", "Y1"]).and_then(|value| parse_year(&value)),
        doi: first(fields, &["DO"]),
        abstract_text: first(fields, &["AB", "N2"]),
        keywords: all(fields, &["KW"]),
        venue: first(fields, &["JO", "JF", "T2", "BT"]),
    }
}

fn parse_bibtex(content: &str) -> AppResult<Vec<ReferenceRecord>> {
    let mut records = Vec::new();
    let bytes = content.as_bytes();
    let mut cursor = 0;
    while let Some(relative) = content[cursor..].find('@') {
        let start = cursor + relative;
        let Some(open_relative) = content[start..].find(['{', '(']) else {
            break;
        };
        let open = start + open_relative;
        let close_char = if bytes[open] == b'{' { b'}' } else { b')' };
        let mut depth = 1_i32;
        let mut index = open + 1;
        while index < bytes.len() && depth > 0 {
            if bytes[index] == bytes[open] {
                depth += 1;
            }
            if bytes[index] == close_char {
                depth -= 1;
            }
            index += 1;
        }
        if depth != 0 {
            return Err(AppError::Validation("Entrada BibTeX sin cierre".into()));
        }
        let body = &content[open + 1..index - 1];
        let Some((key, fields_body)) = body.split_once(',') else {
            cursor = index;
            continue;
        };
        let fields = parse_bibtex_fields(fields_body);
        records.push(ReferenceRecord {
            source_key: Some(key.trim().into()),
            title: fields.get("title").cloned().unwrap_or_default(),
            authors: fields
                .get("author")
                .map(|value| value.split(" and ").map(str::to_owned).collect())
                .unwrap_or_default(),
            year: fields.get("year").and_then(|value| parse_year(value)),
            doi: fields.get("doi").cloned(),
            abstract_text: fields.get("abstract").cloned(),
            keywords: fields
                .get("keywords")
                .map(|value| value.split([',', ';']).map(str::to_owned).collect())
                .unwrap_or_default(),
            venue: fields
                .get("journal")
                .or_else(|| fields.get("booktitle"))
                .cloned(),
        });
        cursor = index;
    }
    if records.is_empty() {
        return Err(AppError::Validation(
            "El archivo BibTeX no contiene entradas reconocibles".into(),
        ));
    }
    Ok(records)
}

fn parse_bibtex_fields(content: &str) -> BTreeMap<String, String> {
    let bytes = content.as_bytes();
    let mut fields = BTreeMap::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        while cursor < bytes.len() && (bytes[cursor].is_ascii_whitespace() || bytes[cursor] == b',')
        {
            cursor += 1;
        }
        let key_start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_alphanumeric() || bytes[cursor] == b'_')
        {
            cursor += 1;
        }
        let key = content[key_start..cursor].trim().to_lowercase();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'=' {
            cursor += 1;
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let value = if cursor < bytes.len() && (bytes[cursor] == b'{' || bytes[cursor] == b'"') {
            let open = bytes[cursor];
            let close = if open == b'{' { b'}' } else { b'"' };
            cursor += 1;
            let start = cursor;
            let mut depth = 1_i32;
            while cursor < bytes.len() && depth > 0 {
                if open == b'{' && bytes[cursor] == open {
                    depth += 1;
                }
                if bytes[cursor] == close {
                    depth -= 1;
                }
                if depth > 0 {
                    cursor += 1;
                }
            }
            let value = content[start..cursor].trim().replace(['{', '}'], "");
            cursor += usize::from(cursor < bytes.len());
            value
        } else {
            let start = cursor;
            while cursor < bytes.len() && bytes[cursor] != b',' {
                cursor += 1;
            }
            content[start..cursor].trim().into()
        };
        if !key.is_empty() {
            fields.insert(key, value);
        }
    }
    fields
}

fn parse_csv(path: &Path) -> AppResult<Vec<ReferenceRecord>> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(path)
        .map_err(|error| AppError::Validation(format!("CSV inválido: {error}")))?;
    let headers = reader
        .headers()
        .map_err(|error| AppError::Validation(format!("Cabecera CSV inválida: {error}")))?
        .clone();
    let normalized = headers.iter().map(normalize_header).collect::<Vec<_>>();
    let mut records = Vec::new();
    for row in reader.records() {
        let row =
            row.map_err(|error| AppError::Validation(format!("Fila CSV inválida: {error}")))?;
        records.push(record_from_csv(&normalized, &row));
    }
    if records.is_empty() {
        return Err(AppError::Validation("El CSV no contiene registros".into()));
    }
    Ok(records)
}

fn record_from_csv(headers: &[String], row: &StringRecord) -> ReferenceRecord {
    let value = |aliases: &[&str]| -> Option<String> {
        headers
            .iter()
            .position(|header| aliases.contains(&header.as_str()))
            .and_then(|index| row.get(index))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    ReferenceRecord {
        source_key: value(&["id", "key"]),
        title: value(&["title", "titulo"]).unwrap_or_default(),
        authors: value(&["authors", "author", "autores"])
            .map(|value| value.split(';').map(str::to_owned).collect())
            .unwrap_or_default(),
        year: value(&["year", "ano", "anio"]).and_then(|value| parse_year(&value)),
        doi: value(&["doi"]),
        abstract_text: value(&["abstract", "resumen"]),
        keywords: value(&["keywords", "palabrasclave"])
            .map(|value| value.split([';', ',']).map(str::to_owned).collect())
            .unwrap_or_default(),
        venue: value(&["venue", "journal", "conference", "revista", "conferencia"]),
    }
}

fn normalize_header(value: &str) -> String {
    value.trim().to_lowercase().replace([' ', '_', '-'], "")
}

fn first(fields: &BTreeMap<String, Vec<String>>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| fields.get(*key).and_then(|values| values.first()).cloned())
}

fn all(fields: &BTreeMap<String, Vec<String>>, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .filter_map(|key| fields.get(*key))
        .flatten()
        .cloned()
        .collect()
}

fn parse_year(value: &str) -> Option<i64> {
    value
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>()
        .get(0..4)?
        .parse()
        .ok()
}

fn normalize_doi(value: &str) -> Option<String> {
    let value = value.trim().to_lowercase();
    let value = value
        .strip_prefix("https://doi.org/")
        .or_else(|| value.strip_prefix("http://doi.org/"))
        .or_else(|| value.strip_prefix("doi:"))
        .unwrap_or(&value)
        .trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn clean_list(values: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for value in values {
        let value = value.trim();
        if !value.is_empty()
            && !result
                .iter()
                .any(|item: &String| item.eq_ignore_ascii_case(value))
        {
            result.push(value.into());
        }
    }
    result
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn imports_ris_and_preserves_source_provenance() {
        let directory = tempdir().expect("temp directory");
        let database = Database::initialize(directory.path().join("db.sqlite3")).expect("database");
        let ris = directory.path().join("references.ris");
        fs::write(&ris, "TY  - JOUR\nTI  - Verifiable Research\nAU  - Rivera, Ana\nPY  - 2025\nDO  - 10.1000/test\nKW  - evidence\nER  -\n").expect("RIS fixture");

        let result = database
            .import_reference_files(vec![ris.to_string_lossy().into_owned()])
            .expect("import");
        assert_eq!(result[0].status, "imported");
        assert_eq!(
            database.library_articles(None).expect("library")[0].title,
            "Verifiable Research"
        );
        let connection = database.open().expect("connection");
        let sources: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM source_records WHERE record_status = 'imported'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(sources, 1);
    }

    #[test]
    fn flags_exact_title_without_doi_instead_of_merging() {
        let directory = tempdir().expect("temp directory");
        let database = Database::initialize(directory.path().join("db.sqlite3")).expect("database");
        for (name, author) in [
            ("first.csv", "One Author"),
            ("second.csv", "Another Author"),
        ] {
            fs::write(
                directory.path().join(name),
                format!("title,year,authors\nSame Study,2024,{author}\n"),
            )
            .expect("CSV fixture");
            database
                .import_reference_files(vec![
                    directory.path().join(name).to_string_lossy().into_owned(),
                ])
                .expect("import");
        }
        assert_eq!(database.library_articles(None).expect("library").len(), 2);
        let connection = database.open().expect("connection");
        let candidates: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM duplicate_candidates WHERE status = 'pending'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(candidates, 1);
    }
}
