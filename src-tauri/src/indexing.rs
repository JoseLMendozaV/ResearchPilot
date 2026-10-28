//! Incremental retrieval index: chunks + FTS5 always, embeddings when an
//! Ollama embedding model is configured and reachable.

use std::time::Duration;

use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::{
    db::{Database, now},
    error::{AppError, AppResult},
    jobs::INDEX_JOB,
    models::{AppSettings, EnqueueResult},
    ollama::OllamaClient,
    rag::{SourceBlock, blocks_from_page_text, chunk_blocks, encode_vector, sha256_hex},
};

const EMBED_BATCH: usize = 16;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub articles: i64,
    pub indexed_articles: i64,
    pub chunks: i64,
    pub embedded_chunks: i64,
    pub embedding_model: String,
}

impl Database {
    /// Rebuild an article's chunks and embed the ones missing for the current model.
    /// Returns a human-readable summary.
    pub fn index_article(
        &self,
        article_id: &str,
        settings: &AppSettings,
        progress: &dyn Fn(f64, &str) -> AppResult<()>,
        cancelled: &dyn Fn() -> bool,
    ) -> AppResult<String> {
        let connection = self.open()?;
        let document: Option<(String, String)> = connection
            .query_row(
                "SELECT d.id, a.title FROM articles a
                 JOIN document_files d ON d.article_id = a.id AND d.is_primary = 1 AND d.deleted_at IS NULL
                 WHERE a.id = ?1 AND a.deleted_at IS NULL ORDER BY d.imported_at DESC LIMIT 1",
                [article_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((document_id, _title)) = document else {
            return Err(AppError::Validation(
                "El artículo no tiene un PDF activo para indexar".into(),
            ));
        };

        let mut blocks: Vec<SourceBlock> = {
            let mut statement = connection.prepare(
                "SELECT p.page_number, b.block_type, b.text_content, b.section_path, b.canonical_section, b.bounding_box_json
                 FROM document_blocks b JOIN document_pages p ON p.id = b.document_page_id
                 WHERE p.document_file_id = ?1 ORDER BY p.page_number, b.reading_order",
            )?;
            statement
                .query_map([&document_id], |row| {
                    let bbox: Option<String> = row.get(5)?;
                    Ok(SourceBlock {
                        page: row.get(0)?,
                        block_type: row.get(1)?,
                        text: row.get(2)?,
                        section: row.get(3)?,
                        canonical: row.get(4)?,
                        bbox: bbox.and_then(|text| serde_json::from_str(&text).ok()),
                    })
                })?
                .collect::<Result<_, _>>()?
        };
        if blocks.is_empty() {
            let mut statement = connection.prepare(
                "SELECT page_number, text_content FROM document_pages WHERE document_file_id = ?1 ORDER BY page_number",
            )?;
            let pages: Vec<(i64, String)> = statement
                .query_map([&document_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;
            blocks = pages
                .iter()
                .flat_map(|(page, text)| blocks_from_page_text(*page, text))
                .collect();
        }
        let chunks = chunk_blocks(&blocks);
        drop(connection);

        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM document_chunks WHERE article_id = ?1",
            [article_id],
        )?;
        let timestamp = now();
        for (index, chunk) in chunks.iter().enumerate() {
            transaction.execute(
                "INSERT INTO document_chunks (id, article_id, document_file_id, chunk_index, page_number, section,
                                              canonical_section, text, spans_json, content_sha256, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    Uuid::new_v4().to_string(),
                    article_id,
                    document_id,
                    index as i64,
                    chunk.page,
                    chunk.section,
                    chunk.canonical,
                    chunk.text,
                    serde_json::to_string(&chunk.spans).map_err(|error| AppError::Ai(error.to_string()))?,
                    sha256_hex(chunk.text.as_bytes()),
                    timestamp,
                ],
            )?;
        }
        transaction.commit()?;
        progress(
            0.2,
            &format!("{} fragmentos con índice léxico", chunks.len()),
        )?;

        let model = settings.ollama_embedding_model.trim();
        if model.is_empty() {
            return Ok(format!(
                "{} fragmentos indexados (léxico). Sin modelo de embeddings configurado.",
                chunks.len()
            ));
        }
        let pending: Vec<(String, String, Option<String>)> = {
            let connection = self.open()?;
            let mut statement = connection.prepare(
                "SELECT DISTINCT c.content_sha256, c.text, c.section FROM document_chunks c
                 WHERE c.article_id = ?1 AND NOT EXISTS (
                    SELECT 1 FROM chunk_embeddings e WHERE e.content_sha256 = c.content_sha256 AND e.model = ?2)",
            )?;
            statement
                .query_map(params![article_id, model], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .collect::<Result<_, _>>()?
        };
        if pending.is_empty() {
            return Ok(format!(
                "{} fragmentos indexados; embeddings ya vigentes para {model}.",
                chunks.len()
            ));
        }
        let client = OllamaClient::from_settings(
            settings,
            Duration::from_secs(settings.ai_timeout_seconds as u64),
        )?;
        let mut done = 0;
        for batch in pending.chunks(EMBED_BATCH) {
            if cancelled() {
                return Err(AppError::Cancelled);
            }
            // Section context improves retrieval; the stored text stays literal.
            let inputs: Vec<String> = batch
                .iter()
                .map(|(_, text, section)| match section {
                    Some(section) if !section.is_empty() => format!("{section}\n{text}"),
                    _ => text.clone(),
                })
                .collect();
            let vectors = match client.embed(model, &inputs) {
                Ok(vectors) => vectors,
                Err(error) if done == 0 => {
                    return Ok(format!(
                        "{} fragmentos indexados (léxico). Embeddings pendientes: {error}",
                        chunks.len()
                    ));
                }
                Err(error) => return Err(error),
            };
            let connection = self.open()?;
            for ((sha, _, _), vector) in batch.iter().zip(&vectors) {
                connection.execute(
                    "INSERT OR REPLACE INTO chunk_embeddings (content_sha256, model, dimensions, vector, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![sha, model, vector.len() as i64, encode_vector(vector), now()],
                )?;
            }
            done += batch.len();
            progress(
                0.2 + 0.8 * done as f64 / pending.len() as f64,
                &format!("Embeddings {done} de {}", pending.len()),
            )?;
        }
        self.open()?.execute(
            "DELETE FROM chunk_embeddings WHERE content_sha256 NOT IN (SELECT content_sha256 FROM document_chunks)",
            [],
        )?;
        Ok(format!(
            "{} fragmentos indexados; {done} embeddings nuevos con {model}.",
            chunks.len()
        ))
    }

    pub fn index_status(&self) -> AppResult<IndexStatus> {
        let settings = self.settings()?;
        let model = settings.ollama_embedding_model.trim().to_owned();
        let connection = self.open()?;
        Ok(IndexStatus {
            articles: connection.query_row(
                "SELECT COUNT(*) FROM articles WHERE deleted_at IS NULL AND pdf_path IS NOT NULL",
                [],
                |row| row.get(0),
            )?,
            indexed_articles: connection.query_row(
                "SELECT COUNT(DISTINCT c.article_id) FROM document_chunks c JOIN articles a ON a.id = c.article_id WHERE a.deleted_at IS NULL",
                [],
                |row| row.get(0),
            )?,
            chunks: connection.query_row("SELECT COUNT(*) FROM document_chunks", [], |row| row.get(0))?,
            embedded_chunks: connection.query_row(
                "SELECT COUNT(*) FROM document_chunks c JOIN chunk_embeddings e ON e.content_sha256 = c.content_sha256 AND e.model = ?1",
                [&model],
                |row| row.get(0),
            )?,
            embedding_model: model,
        })
    }

    /// Queue indexing for the given articles, or for every active article with a PDF.
    pub fn enqueue_indexing(
        &self,
        article_ids: Option<&[String]>,
    ) -> AppResult<Vec<EnqueueResult>> {
        let ids: Vec<String> = match article_ids {
            Some(ids) => ids.to_vec(),
            None => {
                let connection = self.open()?;
                let mut statement = connection.prepare("SELECT id FROM articles WHERE deleted_at IS NULL AND pdf_path IS NOT NULL ORDER BY created_at")?;
                statement
                    .query_map([], |row| row.get(0))?
                    .collect::<Result<_, _>>()?
            }
        };
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        let mut results = Vec::new();
        for article_id in ids {
            let active: Option<String> = transaction
                .query_row(
                    "SELECT id FROM ingestion_jobs WHERE article_id = ?1 AND job_type = ?2 AND status IN ('queued', 'paused') LIMIT 1",
                    params![article_id, INDEX_JOB],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(job_id) = active {
                results.push(EnqueueResult {
                    article_id,
                    status: "already_queued".into(),
                    job_id: Some(job_id),
                    message: None,
                });
                continue;
            }
            let job_id = Uuid::new_v4().to_string();
            let timestamp = now();
            transaction.execute(
                "INSERT INTO ingestion_jobs (id, article_id, job_type, status, progress, input_json, message, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'queued', 0, ?4, 'En cola', ?5, ?5)",
                params![job_id, article_id, INDEX_JOB, json!({}).to_string(), timestamp],
            )?;
            results.push(EnqueueResult {
                article_id,
                status: "queued".into(),
                job_id: Some(job_id),
                message: None,
            });
        }
        transaction.commit()?;
        Ok(results)
    }
}
