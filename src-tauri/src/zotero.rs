//! Import from a local Zotero library (read-only).
//!
//! Zotero keeps its database open while running, so we read a private copy of
//! `zotero.sqlite`. Items become library articles (PDF attachments go through the
//! normal PDF importer), Zotero collections become local collections, and each item
//! is linked by its key so importing again only adds what is new.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::{
    db::{Database, now},
    error::{AppError, AppResult},
    library::ImportOutcome,
};

const MAX_ITEMS: usize = 20_000;
const SKIPPED_TYPES: [&str; 3] = ["attachment", "note", "annotation"];
const VENUE_FIELDS: [&str; 6] = [
    "publicationTitle",
    "proceedingsTitle",
    "bookTitle",
    "conferenceName",
    "university",
    "publisher",
];

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroImportSummary {
    pub items: usize,
    pub imported: usize,
    pub linked_existing: usize,
    pub already_linked: usize,
    pub pdfs_attached: usize,
    pub collections: usize,
    pub errors: Vec<String>,
    #[serde(skip)]
    pub new_pdf_articles: Vec<String>,
}

#[derive(Debug, Default)]
struct ZoteroItem {
    key: String,
    item_type: String,
    fields: HashMap<String, String>,
    authors: Vec<String>,
    tags: Vec<String>,
    collections: Vec<String>,
    pdf: Option<PathBuf>,
}

impl ZoteroItem {
    fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .get(name)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    }

    fn year(&self) -> Option<i64> {
        let date = self.field("date")?;
        date.as_bytes().windows(4).find_map(|window| {
            let text = std::str::from_utf8(window).ok()?;
            let year: i64 = text.parse().ok()?;
            (1000..=2999).contains(&year).then_some(year)
        })
    }

    fn venue(&self) -> Option<&str> {
        VENUE_FIELDS.iter().find_map(|name| self.field(name))
    }

    fn doi(&self) -> Option<String> {
        self.field("DOI").map(|doi| {
            doi.trim_start_matches("https://doi.org/")
                .trim_start_matches("doi:")
                .trim()
                .to_lowercase()
        })
    }
}

/// `~/Zotero`, where Zotero keeps its data by default on every platform.
pub fn default_data_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let directory = PathBuf::from(home).join("Zotero");
    directory
        .join("zotero.sqlite")
        .is_file()
        .then_some(directory)
}

impl Database {
    pub fn import_zotero(
        &self,
        data_dir: &Path,
        include_pdfs: bool,
    ) -> AppResult<ZoteroImportSummary> {
        let source = data_dir.join("zotero.sqlite");
        if !source.is_file() {
            return Err(AppError::Validation(format!(
                "No se encontró zotero.sqlite en {}; elige la carpeta de datos de Zotero (Zotero → Ajustes → Avanzado → Archivos y carpetas)",
                data_dir.display()
            )));
        }
        let copy = self
            .storage_dir
            .join(format!("zotero-{}.sqlite", Uuid::new_v4()));
        fs::copy(&source, &copy)?;
        let wal = data_dir.join("zotero.sqlite-wal");
        if wal.is_file() {
            fs::copy(&wal, copy.with_extension("sqlite-wal"))?;
        }
        let items = read_items(&copy, data_dir);
        for suffix in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(format!("{}{suffix}", copy.to_string_lossy()));
        }
        let (items, library_names) = items?;
        if items.len() > MAX_ITEMS {
            return Err(AppError::Validation(format!(
                "La biblioteca de Zotero supera el límite de {MAX_ITEMS} referencias por importación"
            )));
        }

        let mut summary = ZoteroImportSummary {
            items: items.len(),
            ..Default::default()
        };
        let mut collection_ids: HashMap<String, String> = HashMap::new();
        for item in items {
            let external_key = item.key.clone();
            let title = item.field("title").map(str::to_owned);
            match self.import_zotero_item(&item, include_pdfs, &mut summary) {
                Ok(Some(article_id)) => {
                    for name in &item.collections {
                        let collection = match collection_ids.get(name) {
                            Some(id) => id.clone(),
                            None => {
                                let id = self.collection_named(name)?;
                                collection_ids.insert(name.clone(), id.clone());
                                id
                            }
                        };
                        self.open()?.execute(
                            "INSERT OR IGNORE INTO collection_articles (collection_id, article_id, added_at) VALUES (?1, ?2, ?3)",
                            params![collection, article_id, now()],
                        )?;
                    }
                }
                Ok(None) => {}
                Err(error) => summary
                    .errors
                    .push(format!("{}: {error}", title.unwrap_or(external_key))),
            }
        }
        summary.collections = collection_ids.len();
        self.open()?.execute(
            "INSERT INTO audit_events (id, entity_type, entity_id, action, actor_type, actor_name, after_json, created_at)
             VALUES (?1, 'library', 'zotero', 'zotero_imported', 'import', 'zotero', ?2, ?3)",
            params![
                Uuid::new_v4().to_string(),
                json!({ "dataDir": data_dir.to_string_lossy(), "libraries": library_names, "summary": summary }).to_string(),
                now(),
            ],
        )?;
        Ok(summary)
    }

    /// Returns the linked article when the item is new to this library.
    fn import_zotero_item(
        &self,
        item: &ZoteroItem,
        include_pdfs: bool,
        summary: &mut ZoteroImportSummary,
    ) -> AppResult<Option<String>> {
        let connection = self.open()?;
        let linked: Option<String> = connection
            .query_row(
                "SELECT article_id FROM external_links WHERE source = 'zotero' AND external_key = ?1",
                [&item.key],
                |row| row.get(0),
            )
            .optional()?;
        if linked.is_some() {
            summary.already_linked += 1;
            return Ok(None);
        }
        let title = item
            .field("title")
            .ok_or_else(|| AppError::Validation("referencia sin título".into()))?;
        let doi = item.doi();

        let mut article_id = None;
        if let (true, Some(pdf)) = (include_pdfs, &item.pdf) {
            match self.import_pdf(&pdf.to_string_lossy()) {
                Ok(ImportOutcome::Imported(article)) => {
                    self.apply_zotero_metadata(&article.id, item, doi.as_deref())?;
                    summary.imported += 1;
                    summary.pdfs_attached += 1;
                    summary.new_pdf_articles.push(article.id.clone());
                    article_id = Some(article.id);
                }
                Ok(ImportOutcome::Duplicate(article)) => {
                    summary.linked_existing += 1;
                    article_id = Some(article.id);
                }
                Err(error) => summary.errors.push(format!(
                    "{title}: PDF no importado ({error}); se importan solo los metadatos"
                )),
            }
        }
        if article_id.is_none() {
            let existing: Option<String> = connection
                .query_row(
                    "SELECT id FROM articles WHERE deleted_at IS NULL AND (
                        (?1 IS NOT NULL AND lower(doi) = ?1)
                        OR (lower(trim(title)) = lower(trim(?2)) AND publication_year IS ?3))
                     ORDER BY (lower(doi) IS ?1) DESC LIMIT 1",
                    params![doi, title, item.year()],
                    |row| row.get(0),
                )
                .optional()?;
            article_id = Some(match existing {
                Some(id) => {
                    summary.linked_existing += 1;
                    id
                }
                None => {
                    let id = Uuid::new_v4().to_string();
                    connection.execute(
                        "INSERT INTO articles (id, title, metadata_source, created_at, updated_at) VALUES (?1, ?2, 'zotero', ?3, ?3)",
                        params![id, title, now()],
                    )?;
                    self.apply_zotero_metadata(&id, item, doi.as_deref())?;
                    summary.imported += 1;
                    id
                }
            });
        }
        let article_id = article_id.expect("set above");
        connection.execute(
            "INSERT INTO external_links (source, external_key, article_id, metadata_json, created_at, updated_at)
             VALUES ('zotero', ?1, ?2, ?3, ?4, ?4)",
            params![
                item.key,
                article_id,
                json!({ "itemType": item.item_type, "fields": item.fields }).to_string(),
                now()
            ],
        )?;
        Ok(Some(article_id))
    }

    fn apply_zotero_metadata(
        &self,
        article_id: &str,
        item: &ZoteroItem,
        doi: Option<&str>,
    ) -> AppResult<()> {
        let connection = self.open()?;
        // DOIs are unique; one already used by another record stays only in the link.
        let doi_free = match doi {
            Some(doi) => !connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM articles WHERE lower(doi) = ?1 AND id <> ?2)",
                params![doi, article_id],
                |row| row.get::<_, bool>(0),
            )?,
            None => false,
        };
        connection.execute(
            "UPDATE articles SET title = ?1, abstract_text = COALESCE(?2, abstract_text), publication_year = COALESCE(?3, publication_year),
                    doi = CASE WHEN ?4 IS NOT NULL THEN ?4 ELSE doi END, authors_json = ?5, keywords_json = ?6,
                    venue = COALESCE(?7, venue), metadata_source = 'zotero', updated_at = ?8
             WHERE id = ?9",
            params![
                item.field("title"),
                item.field("abstractNote"),
                item.year(),
                if doi_free { doi } else { None },
                serde_json::to_string(&item.authors).map_err(json_error)?,
                serde_json::to_string(&item.tags).map_err(json_error)?,
                item.venue(),
                now(),
                article_id,
            ],
        )?;
        connection.execute(
            "INSERT INTO audit_events (id, entity_type, entity_id, action, actor_type, actor_name, after_json, created_at)
             VALUES (?1, 'article', ?2, 'metadata_imported', 'import', 'zotero', ?3, ?4)",
            params![
                Uuid::new_v4().to_string(),
                article_id,
                json!({ "zoteroKey": item.key, "title": item.field("title"), "doi": doi }).to_string(),
                now()
            ],
        )?;
        Ok(())
    }

    fn collection_named(&self, name: &str) -> AppResult<String> {
        let connection = self.open()?;
        let existing: Option<String> = connection
            .query_row(
                "SELECT id FROM collections WHERE name = ?1 AND deleted_at IS NULL",
                [name],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok(id);
        }
        let id = Uuid::new_v4().to_string();
        connection.execute(
            "INSERT INTO collections (id, name, description, created_at) VALUES (?1, ?2, 'Importada de Zotero', ?3)",
            params![id, name, now()],
        )?;
        Ok(id)
    }
}

type Items = (Vec<ZoteroItem>, Vec<String>);

fn read_items(copy: &Path, data_dir: &Path) -> AppResult<Items> {
    let connection = Connection::open_with_flags(copy, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|error| AppError::Validation(format!("zotero.sqlite ilegible: {error}")))?;
    let has_table = |name: &str| -> AppResult<bool> {
        Ok(connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [name],
            |row| row.get(0),
        )?)
    };
    for table in ["items", "itemTypes", "itemData", "itemDataValues", "fields"] {
        if !has_table(table)? {
            return Err(AppError::Validation(
                "El archivo no tiene la estructura de una base de datos de Zotero".into(),
            ));
        }
    }
    let deleted = if has_table("deletedItems")? {
        "SELECT itemID FROM deletedItems"
    } else {
        "SELECT NULL WHERE 0"
    };
    let fields_table = if has_table("fieldsCombined")? {
        "fieldsCombined"
    } else {
        "fields"
    };

    let mut items: Vec<ZoteroItem> = Vec::new();
    let mut index: HashMap<i64, usize> = HashMap::new();
    {
        let mut statement = connection.prepare(&format!(
            "SELECT i.itemID, i.libraryID, i.key, t.typeName FROM items i JOIN itemTypes t ON t.itemTypeID = i.itemTypeID
             WHERE i.itemID NOT IN ({deleted}) ORDER BY i.itemID"
        ))?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (item_id, library_id, key, item_type) = row?;
            if SKIPPED_TYPES.contains(&item_type.as_str()) {
                continue;
            }
            index.insert(item_id, items.len());
            items.push(ZoteroItem {
                key: format!("{library_id}:{key}"),
                item_type,
                ..Default::default()
            });
        }
    }
    let mut each =
        |sql: &str, apply: &mut dyn FnMut(&mut ZoteroItem, Vec<Option<String>>)| -> AppResult<()> {
            let mut statement = connection.prepare(sql)?;
            let width = statement.column_count();
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let item_id: i64 = row.get(0)?;
                if let Some(&position) = index.get(&item_id) {
                    let values = (1..width)
                        .map(|column| {
                            row.get::<_, Option<rusqlite::types::Value>>(column)
                                .map(|value| match value {
                                    Some(rusqlite::types::Value::Text(text)) => Some(text),
                                    Some(rusqlite::types::Value::Integer(number)) => {
                                        Some(number.to_string())
                                    }
                                    Some(rusqlite::types::Value::Real(number)) => {
                                        Some(number.to_string())
                                    }
                                    _ => None,
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    apply(&mut items[position], values);
                }
            }
            Ok(())
        };
    each(
        &format!(
            "SELECT d.itemID, f.fieldName, v.value FROM itemData d
             JOIN {fields_table} f ON f.fieldID = d.fieldID JOIN itemDataValues v ON v.valueID = d.valueID"
        ),
        &mut |item, values| {
            if let (Some(name), Some(value)) = (values[0].clone(), values[1].clone()) {
                item.fields.insert(name, value);
            }
        },
    )?;
    if has_table("itemCreators")? && has_table("creators")? {
        each(
            "SELECT ic.itemID, c.firstName, c.lastName, c.fieldMode FROM itemCreators ic
             JOIN creators c ON c.creatorID = ic.creatorID ORDER BY ic.itemID, ic.orderIndex",
            &mut |item, values| {
                let first = values[0].clone().unwrap_or_default();
                let last = values[1].clone().unwrap_or_default();
                let name = if values[2].as_deref() == Some("1") || first.trim().is_empty() {
                    last.trim().to_owned()
                } else {
                    format!("{}, {}", last.trim(), first.trim())
                };
                if !name.is_empty() {
                    item.authors.push(name);
                }
            },
        )?;
    }
    if has_table("itemTags")? && has_table("tags")? {
        each(
            "SELECT it.itemID, t.name FROM itemTags it JOIN tags t ON t.tagID = it.tagID ORDER BY t.name",
            &mut |item, values| {
                if let Some(tag) = values[0].clone().filter(|tag| !tag.trim().is_empty()) {
                    item.tags.push(tag);
                }
            },
        )?;
    }
    if has_table("collections")? && has_table("collectionItems")? {
        let mut names: HashMap<i64, (String, Option<i64>)> = HashMap::new();
        let mut statement = connection
            .prepare("SELECT collectionID, collectionName, parentCollectionID FROM collections")?;
        for row in statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))? {
            let (id, name, parent): (i64, String, Option<i64>) = row?;
            names.insert(id, (name, parent));
        }
        let path = |mut id: i64| {
            let mut parts = Vec::new();
            while let Some((name, parent)) = names.get(&id) {
                parts.push(name.clone());
                match parent {
                    Some(parent) if parts.len() < 32 => id = *parent,
                    _ => break,
                }
            }
            parts.reverse();
            parts.join(" / ")
        };
        let deleted_collections = if has_table("deletedCollections")? {
            "SELECT collectionID FROM deletedCollections"
        } else {
            "SELECT NULL WHERE 0"
        };
        each(
            &format!(
                "SELECT itemID, collectionID FROM collectionItems WHERE collectionID NOT IN ({deleted_collections})"
            ),
            &mut |item, values| {
                if let Some(id) = values[0].as_deref().and_then(|id| id.parse().ok()) {
                    item.collections.push(path(id));
                }
            },
        )?;
    }
    if has_table("itemAttachments")? {
        each(
            &format!(
                "SELECT a.parentItemID, i.key, a.path, a.linkMode FROM itemAttachments a JOIN items i ON i.itemID = a.itemID
                 WHERE a.parentItemID IS NOT NULL AND a.contentType = 'application/pdf' AND a.itemID NOT IN ({deleted})
                 ORDER BY a.itemID"
            ),
            &mut |item, values| {
                if item.pdf.is_some() {
                    return;
                }
                let (Some(key), Some(path)) = (values[0].as_deref(), values[1].as_deref()) else {
                    return;
                };
                let candidate = match path.strip_prefix("storage:") {
                    Some(file) => data_dir.join("storage").join(key).join(file),
                    // Linked files: absolute paths only (relative ones depend on a Zotero preference).
                    None if Path::new(path).is_absolute() => PathBuf::from(path),
                    None => return,
                };
                if candidate.is_file() {
                    item.pdf = Some(candidate);
                }
            },
        )?;
    }
    let mut libraries = Vec::new();
    if has_table("groups")? {
        let mut statement = connection.prepare("SELECT name FROM groups ORDER BY name")?;
        for name in statement.query_map([], |row| row.get::<_, String>(0))? {
            libraries.push(name?);
        }
    }
    Ok((items, libraries))
}

fn json_error(error: serde_json::Error) -> AppError {
    AppError::Initialization(error.to_string())
}

#[cfg(test)]
#[path = "zotero_tests.rs"]
mod tests;
