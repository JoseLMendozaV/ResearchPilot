use std::{
    fs::{self, File},
    io::{BufReader, Read},
    path::Path,
};

use lopdf::Document;
use rusqlite::{Connection, OptionalExtension, Row, params};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    db::{Database, now},
    error::{AppError, AppResult},
    models::{ImportItemResult, LibraryArticle, LibraryCollection, UpdateArticleMetadataInput},
};

const MAX_IMPORT_FILES: usize = 100;
const MAX_PDF_BYTES: u64 = 200 * 1024 * 1024;
const MAX_PAGE_DECOMPRESSED_BYTES: usize = 8 * 1024 * 1024;

impl Database {
    pub fn import_pdf_files(&self, paths: Vec<String>) -> AppResult<Vec<ImportItemResult>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        if paths.len() > MAX_IMPORT_FILES {
            return Err(AppError::Validation(format!(
                "Se permiten como máximo {MAX_IMPORT_FILES} archivos por lote"
            )));
        }

        Ok(paths
            .into_iter()
            .map(|source_path| match self.import_pdf(&source_path) {
                Ok(ImportOutcome::Imported(article)) => ImportItemResult {
                    source_path,
                    status: "imported".into(),
                    article_id: Some(article.id),
                    title: Some(article.title),
                    message: None,
                },
                Ok(ImportOutcome::Duplicate(article)) => ImportItemResult {
                    source_path,
                    status: "duplicate".into(),
                    article_id: Some(article.id),
                    title: Some(article.title),
                    message: Some("Ya existe un documento con la misma huella SHA-256".into()),
                },
                Err(error) => ImportItemResult {
                    source_path,
                    status: "error".into(),
                    article_id: None,
                    title: None,
                    message: Some(error.to_string()),
                },
            })
            .collect())
    }

    fn import_pdf(&self, source_path: &str) -> AppResult<ImportOutcome> {
        let source = fs::canonicalize(source_path)?;
        validate_pdf(&source)?;
        let hash = sha256_file(&source)?;

        if let Some(article) = self.article_by_hash(&hash)? {
            return Ok(ImportOutcome::Duplicate(article));
        }

        let metadata = Document::load_metadata(&source)
            .map_err(|error| AppError::Validation(format!("PDF ilegible o corrupto: {error}")))?;
        if metadata.encrypted {
            return Err(AppError::Validation(
                "El PDF está cifrado; desbloquéalo antes de importarlo".into(),
            ));
        }
        let document = Document::load(&source).map_err(|error| {
            AppError::Validation(format!("No se pudo procesar el PDF: {error}"))
        })?;

        let article_id = Uuid::new_v4().to_string();
        let document_id = Uuid::new_v4().to_string();
        let job_id = Uuid::new_v4().to_string();
        let article_dir = self.storage_dir.join("library").join(&article_id);
        fs::create_dir_all(&article_dir)?;
        let destination = article_dir.join("original.pdf");
        fs::copy(&source, &destination)?;

        let file_name = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document.pdf".into());
        let fallback_title = source
            .file_stem()
            .map(|name| name.to_string_lossy().replace(['_', '-'], " "))
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "Documento sin título".into());
        let title = metadata
            .title
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(fallback_title);
        let authors = metadata
            .author
            .map(|value| split_metadata_list(&value, ';'))
            .unwrap_or_default();
        let keywords = metadata
            .keywords
            .map(|value| split_keywords(&value))
            .unwrap_or_default();
        let publication_year = metadata.creation_date.as_deref().and_then(pdf_year);
        let timestamp = now();
        let byte_size = fs::metadata(&destination)?.len() as i64;

        let import_result = (|| -> AppResult<()> {
            let mut connection = self.open()?;
            let transaction = connection.transaction()?;
            transaction.execute(
                "INSERT INTO articles (
                    id, title, abstract_text, publication_year, authors_json, keywords_json,
                    pdf_path, document_sha256, metadata_source, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pdf_metadata', ?9, ?9)",
                params![
                    article_id,
                    title.trim(),
                    metadata.subject.filter(|value| !value.trim().is_empty()),
                    publication_year,
                    serde_json::to_string(&authors)
                        .map_err(|error| AppError::Initialization(error.to_string()))?,
                    serde_json::to_string(&keywords)
                        .map_err(|error| AppError::Initialization(error.to_string()))?,
                    destination.to_string_lossy(),
                    hash,
                    timestamp,
                ],
            )?;
            transaction.execute(
                "INSERT INTO document_files (
                    id, article_id, original_path, file_name, sha256, byte_size,
                    version_label, is_primary, imported_at, source_path
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'original', 1, ?7, ?8)",
                params![
                    document_id,
                    article_id,
                    destination.to_string_lossy(),
                    file_name,
                    hash,
                    byte_size,
                    timestamp,
                    source.to_string_lossy(),
                ],
            )?;
            transaction.execute(
                "INSERT INTO ingestion_jobs (
                    id, article_id, job_type, status, progress, input_json, created_at, updated_at
                 ) VALUES (?1, ?2, 'pdf_text', 'running', 0, '{}', ?3, ?3)",
                params![job_id, article_id, timestamp],
            )?;

            let pages = document.get_pages();
            let total = pages.len().max(1) as f64;
            for (index, page_number) in pages.keys().copied().enumerate() {
                let text = document
                    .extract_text_with_limit(&[page_number], MAX_PAGE_DECOMPRESSED_BYTES)
                    .unwrap_or_default();
                let status = if text.trim().is_empty() {
                    "ocr_required"
                } else {
                    "extracted"
                };
                transaction.execute(
                    "INSERT INTO document_pages (
                        id, document_file_id, page_number, text_content, extraction_status
                     ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        Uuid::new_v4().to_string(),
                        document_id,
                        page_number,
                        text.trim(),
                        status,
                    ],
                )?;
                transaction.execute(
                    "UPDATE ingestion_jobs SET progress = ?1, updated_at = ?2 WHERE id = ?3",
                    params![(index + 1) as f64 / total, now(), job_id],
                )?;
            }
            transaction.execute(
                "UPDATE ingestion_jobs
                 SET status = 'completed', progress = 1, updated_at = ?1 WHERE id = ?2",
                params![now(), job_id],
            )?;
            transaction.execute(
                "INSERT INTO audit_events (
                    id, entity_type, entity_id, action, actor_type, actor_name, after_json, created_at
                 ) VALUES (?1, 'article', ?2, 'imported', 'import', 'pdf-importer', ?3, ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    article_id,
                    serde_json::json!({ "sha256": hash, "fileName": file_name }).to_string(),
                    now(),
                ],
            )?;
            transaction.commit()?;
            Ok(())
        })();

        if let Err(error) = import_result {
            let _ = fs::remove_dir_all(&article_dir);
            return Err(error);
        }

        Ok(ImportOutcome::Imported(self.article_by_id(&article_id)?))
    }

    pub fn library_articles(&self, query: Option<&str>) -> AppResult<Vec<LibraryArticle>> {
        let connection = self.open()?;
        let base = library_select();
        let trimmed = query.unwrap_or_default().trim();
        if trimmed.is_empty() {
            let mut statement = connection.prepare(&format!(
                "{base} WHERE a.deleted_at IS NULL ORDER BY a.updated_at DESC LIMIT 500"
            ))?;
            return collect_articles(&mut statement, []);
        }

        let fts_query = fts_query(trimmed)?;
        let mut statement = connection.prepare(&format!(
            "{base}
             JOIN articles_fts ON articles_fts.rowid = a.rowid
             WHERE a.deleted_at IS NULL AND articles_fts MATCH ?1
             ORDER BY rank LIMIT 500"
        ))?;
        collect_articles(&mut statement, [fts_query])
    }

    pub fn update_article_metadata(
        &self,
        input: UpdateArticleMetadataInput,
    ) -> AppResult<LibraryArticle> {
        let title = input.title.trim();
        if title.len() < 2 || title.len() > 500 {
            return Err(AppError::Validation(
                "El título debe tener entre 2 y 500 caracteres".into(),
            ));
        }
        if let Some(year) = input.publication_year
            && !(1000..=2200).contains(&year)
        {
            return Err(AppError::Validation("El año no es válido".into()));
        }
        let before = self.article_by_id(&input.id)?;
        let authors = clean_list(input.authors);
        let keywords = clean_list(input.keywords);
        let connection = self.open()?;
        connection.execute(
            "UPDATE articles SET
                title = ?1, abstract_text = ?2, publication_year = ?3, doi = ?4,
                authors_json = ?5, keywords_json = ?6, venue = ?7, updated_at = ?8
             WHERE id = ?9 AND deleted_at IS NULL",
            params![
                title,
                clean_optional(input.abstract_text),
                input.publication_year,
                clean_optional(input.doi),
                serde_json::to_string(&authors)
                    .map_err(|error| AppError::Initialization(error.to_string()))?,
                serde_json::to_string(&keywords)
                    .map_err(|error| AppError::Initialization(error.to_string()))?,
                clean_optional(input.venue),
                now(),
                input.id,
            ],
        )?;
        let after = self.article_by_id(&input.id)?;
        connection.execute(
            "INSERT INTO audit_events (
                id, entity_type, entity_id, action, actor_type, actor_name,
                before_json, after_json, created_at
             ) VALUES (?1, 'article', ?2, 'metadata_updated', 'human', 'local-user', ?3, ?4, ?5)",
            params![
                Uuid::new_v4().to_string(),
                input.id,
                serde_json::to_string(&before)
                    .map_err(|error| AppError::Initialization(error.to_string()))?,
                serde_json::to_string(&after)
                    .map_err(|error| AppError::Initialization(error.to_string()))?,
                now(),
            ],
        )?;
        Ok(after)
    }

    pub fn archive_article(&self, article_id: &str) -> AppResult<()> {
        let connection = self.open()?;
        let changed = connection.execute(
            "UPDATE articles SET deleted_at = ?1, updated_at = ?1
             WHERE id = ?2 AND deleted_at IS NULL",
            params![now(), article_id],
        )?;
        if changed == 0 {
            return Err(AppError::Validation("El artículo no existe".into()));
        }
        connection.execute(
            "INSERT INTO audit_events (
                id, entity_type, entity_id, action, actor_type, actor_name, created_at
             ) VALUES (?1, 'article', ?2, 'archived', 'human', 'local-user', ?3)",
            params![Uuid::new_v4().to_string(), article_id, now()],
        )?;
        Ok(())
    }

    pub fn library_collections(&self) -> AppResult<Vec<LibraryCollection>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT c.id, c.name, c.description,
                    COUNT(CASE WHEN a.deleted_at IS NULL THEN ca.article_id END), c.created_at
             FROM collections c
             LEFT JOIN collection_articles ca ON ca.collection_id = c.id
             LEFT JOIN articles a ON a.id = ca.article_id
             WHERE c.deleted_at IS NULL
             GROUP BY c.id ORDER BY lower(c.name)",
        )?;
        Ok(statement
            .query_map([], |row| {
                Ok(LibraryCollection {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    article_count: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn create_library_collection(&self, name: &str) -> AppResult<LibraryCollection> {
        let name = name.trim();
        if name.len() < 2 || name.len() > 80 {
            return Err(AppError::Validation(
                "El nombre de la colección debe tener entre 2 y 80 caracteres".into(),
            ));
        }
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        let connection = self.open()?;
        connection.execute(
            "INSERT INTO collections (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![id, name, timestamp],
        )?;
        Ok(LibraryCollection {
            id,
            name: name.into(),
            description: String::new(),
            article_count: 0,
            created_at: timestamp,
        })
    }

    pub fn set_article_collection(
        &self,
        article_id: &str,
        collection_id: &str,
        included: bool,
    ) -> AppResult<()> {
        let connection = self.open()?;
        let article_exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM articles WHERE id = ?1 AND deleted_at IS NULL)",
            [article_id],
            |row| row.get(0),
        )?;
        let collection_exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM collections WHERE id = ?1 AND deleted_at IS NULL)",
            [collection_id],
            |row| row.get(0),
        )?;
        if !article_exists || !collection_exists {
            return Err(AppError::Validation(
                "Artículo o colección inexistente".into(),
            ));
        }
        if included {
            connection.execute(
                "INSERT OR IGNORE INTO collection_articles (collection_id, article_id, added_at)
                 VALUES (?1, ?2, ?3)",
                params![collection_id, article_id, now()],
            )?;
        } else {
            connection.execute(
                "DELETE FROM collection_articles WHERE collection_id = ?1 AND article_id = ?2",
                params![collection_id, article_id],
            )?;
        }
        connection.execute(
            "INSERT INTO audit_events
             (id, entity_type, entity_id, action, actor_type, actor_name, after_json, created_at)
             VALUES (?1, 'article', ?2, ?3, 'human', 'local-user', ?4, ?5)",
            params![
                Uuid::new_v4().to_string(),
                article_id,
                if included {
                    "collection_added"
                } else {
                    "collection_removed"
                },
                serde_json::json!({ "collectionId": collection_id }).to_string(),
                now()
            ],
        )?;
        Ok(())
    }

    fn article_by_hash(&self, hash: &str) -> AppResult<Option<LibraryArticle>> {
        let connection = self.open()?;
        article_query_one(&connection, "a.document_sha256 = ?1", hash, true)
    }

    pub(crate) fn article_by_id(&self, article_id: &str) -> AppResult<LibraryArticle> {
        let connection = self.open()?;
        article_query_one(&connection, "a.id = ?1", article_id, false)?
            .ok_or_else(|| AppError::Validation("El artículo no existe".into()))
    }
}

enum ImportOutcome {
    Imported(LibraryArticle),
    Duplicate(LibraryArticle),
}

fn validate_pdf(path: &Path) -> AppResult<()> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AppError::Validation("La ruta no es un archivo".into()));
    }
    if metadata.len() == 0 || metadata.len() > MAX_PDF_BYTES {
        return Err(AppError::Validation(format!(
            "El PDF debe tener entre 1 byte y {} MB",
            MAX_PDF_BYTES / 1024 / 1024
        )));
    }
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_lowercase)
        .as_deref()
        != Some("pdf")
    {
        return Err(AppError::Validation(
            "El archivo no tiene extensión PDF".into(),
        ));
    }
    let mut file = File::open(path)?;
    let mut signature = [0_u8; 5];
    file.read_exact(&mut signature)?;
    if &signature != b"%PDF-" {
        return Err(AppError::Validation(
            "La firma del archivo no corresponde a un PDF".into(),
        ));
    }
    Ok(())
}

pub(crate) fn sha256_file(path: &Path) -> AppResult<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn library_select() -> &'static str {
    "SELECT
        a.id, a.title, a.abstract_text, a.publication_year, a.doi,
        a.authors_json, a.keywords_json, a.venue,
        d.original_path, d.file_name, d.sha256, d.byte_size,
        COALESCE((SELECT COUNT(*) FROM document_pages p WHERE p.document_file_id = d.id), 0),
        COALESCE(d.imported_at, a.created_at), a.updated_at,
        COALESCE((SELECT json_group_array(ca.collection_id)
                  FROM collection_articles ca WHERE ca.article_id = a.id), '[]')
     FROM articles a
     LEFT JOIN document_files d ON d.id = (
        SELECT df.id FROM document_files df
        WHERE df.article_id = a.id AND df.is_primary = 1 AND df.deleted_at IS NULL
        ORDER BY df.imported_at DESC LIMIT 1
     )"
}

fn article_query_one(
    connection: &Connection,
    predicate: &str,
    value: &str,
    include_archived: bool,
) -> AppResult<Option<LibraryArticle>> {
    let archive_filter = if include_archived {
        ""
    } else {
        " AND a.deleted_at IS NULL"
    };
    let query = format!(
        "{} WHERE {predicate}{archive_filter} LIMIT 1",
        library_select()
    );
    connection
        .query_row(&query, [value], map_article)
        .optional()
        .map_err(Into::into)
}

fn collect_articles<P>(
    statement: &mut rusqlite::Statement<'_>,
    params: P,
) -> AppResult<Vec<LibraryArticle>>
where
    P: rusqlite::Params,
{
    Ok(statement
        .query_map(params, map_article)?
        .collect::<Result<Vec<_>, _>>()?)
}

fn map_article(row: &Row<'_>) -> rusqlite::Result<LibraryArticle> {
    let authors_json: String = row.get(5)?;
    let keywords_json: String = row.get(6)?;
    Ok(LibraryArticle {
        id: row.get(0)?,
        title: row.get(1)?,
        abstract_text: row.get(2)?,
        publication_year: row.get(3)?,
        doi: row.get(4)?,
        authors: serde_json::from_str(&authors_json).unwrap_or_default(),
        keywords: serde_json::from_str(&keywords_json).unwrap_or_default(),
        venue: row.get(7)?,
        document_path: row.get(8)?,
        file_name: row.get(9)?,
        document_sha256: row.get(10)?,
        byte_size: row.get(11)?,
        page_count: row.get(12)?,
        imported_at: row.get(13)?,
        updated_at: row.get(14)?,
        collection_ids: serde_json::from_str(&row.get::<_, String>(15)?).unwrap_or_default(),
    })
}

fn fts_query(value: &str) -> AppResult<String> {
    let terms = value
        .split_whitespace()
        .map(|term| term.trim_matches(|character: char| !character.is_alphanumeric()))
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return Err(AppError::Validation(
            "Escribe al menos un término de búsqueda".into(),
        ));
    }
    Ok(terms.join(" AND "))
}

fn split_metadata_list(value: &str, separator: char) -> Vec<String> {
    clean_list(value.split(separator).map(str::to_owned).collect())
}

fn split_keywords(value: &str) -> Vec<String> {
    clean_list(value.split([',', ';']).map(str::to_owned).collect())
}

fn clean_list(values: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for value in values {
        let value = value.trim();
        if !value.is_empty()
            && !result
                .iter()
                .any(|current: &String| current.eq_ignore_ascii_case(value))
        {
            result.push(value.to_owned());
        }
    }
    result
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn pdf_year(value: &str) -> Option<i64> {
    let digits = value.strip_prefix("D:").unwrap_or(value);
    digits.get(0..4)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use lopdf::{
        Object, Stream,
        content::{Content, Operation},
        dictionary,
    };
    use tempfile::tempdir;

    use super::*;

    fn create_pdf(path: &Path) {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"
        });
        let resources_id = document.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id }
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Td", vec![50.into(), 700.into()]),
                Operation::new("Tj", vec![Object::string_literal("Evidence page")]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id =
            document.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content_id
        });
        document.objects.insert(pages_id, Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            "Resources" => resources_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()]
        }));
        let catalog_id =
            document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        let info_id =
            document.add_object(dictionary! { "Title" => Object::string_literal("Fixture Paper") });
        document.trailer.set("Root", catalog_id);
        document.trailer.set("Info", info_id);
        document.save(path).expect("save fixture PDF");
    }

    #[test]
    fn imports_pdf_extracts_page_and_detects_duplicate_hash() {
        let directory = tempdir().expect("temp directory");
        let database =
            Database::initialize(directory.path().join("researchpilot.sqlite3")).expect("database");
        let pdf = directory.path().join("fixture.pdf");
        create_pdf(&pdf);

        let first = database
            .import_pdf_files(vec![pdf.to_string_lossy().into_owned()])
            .expect("first import");
        assert_eq!(first[0].status, "imported");
        let articles = database.library_articles(None).expect("articles");
        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].title, "Fixture Paper");
        assert_eq!(articles[0].page_count, 1);

        let article_id = articles[0].id.clone();
        database
            .update_article_metadata(UpdateArticleMetadataInput {
                id: article_id.clone(),
                title: "Updated Evidence Study".into(),
                abstract_text: Some("A searchable scientific abstract".into()),
                publication_year: Some(2026),
                doi: Some("10.1000/updated".into()),
                authors: vec!["Ana Researcher".into()],
                keywords: vec!["traceability".into()],
                venue: Some("Evidence Journal".into()),
            })
            .expect("metadata update");
        let search = database
            .library_articles(Some("traceability"))
            .expect("FTS search");
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].title, "Updated Evidence Study");

        let collection = database
            .create_library_collection("Core studies")
            .expect("collection");
        database
            .set_article_collection(&article_id, &collection.id, true)
            .expect("assign collection");
        assert_eq!(
            database.library_articles(None).expect("library")[0].collection_ids,
            vec![collection.id]
        );

        let duplicate = database
            .import_pdf_files(vec![pdf.to_string_lossy().into_owned()])
            .expect("duplicate import");
        assert_eq!(duplicate[0].status, "duplicate");
        assert_eq!(database.stats().expect("stats").articles, 1);

        database.archive_article(&article_id).expect("archive");
        assert!(database.library_articles(None).expect("library").is_empty());
    }

    #[test]
    fn rejects_a_fake_pdf_without_mutating_library() {
        let directory = tempdir().expect("temp directory");
        let database =
            Database::initialize(directory.path().join("researchpilot.sqlite3")).expect("database");
        let fake = directory.path().join("fake.pdf");
        fs::write(&fake, b"not a PDF").expect("fake file");

        let result = database
            .import_pdf_files(vec![fake.to_string_lossy().into_owned()])
            .expect("batch result");
        assert_eq!(result[0].status, "error");
        assert!(
            database
                .library_articles(None)
                .expect("articles")
                .is_empty()
        );
    }
}
