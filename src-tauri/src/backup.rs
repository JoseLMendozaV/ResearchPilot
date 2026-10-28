//! Full local backups (database + managed PDFs) and staged restores.
//!
//! A backup is a zip with a consistent copy of the database (`VACUUM INTO`), every
//! file under `library/` and a manifest holding the SHA-256 of each entry. Restoring
//! never touches the open database: the archive is verified and unpacked into
//! `restore-pending/`, and [`apply_pending_restore`] swaps it in at the next start,
//! before the database is opened.

use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::{
    db::{Database, SCHEMA_VERSION, now},
    error::{AppError, AppResult},
    library::sha256_file,
};

pub const BACKUP_FORMAT: &str = "researchpilot-backup/1";
pub const DATABASE_FILE: &str = "researchpilot.sqlite3";
const MANIFEST_ENTRY: &str = "manifest.json";
const PENDING_DIR: &str = "restore-pending";
const READY_MARKER: &str = "READY";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub format: String,
    pub app_version: String,
    pub schema_version: i64,
    pub created_at: String,
    /// Data folder of the machine that made the backup; stored paths are rebased from it.
    pub storage_dir: String,
    pub articles: i64,
    pub projects: i64,
    pub files: Vec<BackupEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub files: usize,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub created_at: String,
    pub app_version: String,
    pub schema_version: i64,
    pub articles: i64,
    pub projects: i64,
    pub files: usize,
    pub bytes: u64,
    pub pre_restore_backup: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOutcome {
    pub source_sha256: String,
    pub backup_created_at: String,
    pub pre_restore_backup: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub schema_version: i64,
    pub supported_schema_version: i64,
    pub integrity: String,
    pub foreign_key_violations: i64,
    pub database_bytes: u64,
    pub library_files: usize,
    pub library_bytes: u64,
    pub articles: i64,
    pub projects: i64,
    pub missing_pdfs: i64,
    pub missing_pdf_titles: Vec<String>,
    pub pending_restore: bool,
}

impl Database {
    pub fn create_backup(&self, destination: &Path) -> AppResult<BackupResult> {
        if destination.is_dir() {
            return Err(AppError::Validation(
                "Elige un archivo .zip, no una carpeta".into(),
            ));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let snapshot = self
            .storage_dir
            .join(format!("backup-{}.sqlite3", Uuid::new_v4()));
        let partial = destination.with_extension("zip.partial");
        let result = self.write_backup(&snapshot, &partial, destination);
        let _ = fs::remove_file(&snapshot);
        if result.is_err() {
            let _ = fs::remove_file(&partial);
        }
        result
    }

    fn write_backup(
        &self,
        snapshot: &Path,
        partial: &Path,
        destination: &Path,
    ) -> AppResult<BackupResult> {
        let connection = self.open()?;
        connection.execute("VACUUM INTO ?1", [snapshot.to_string_lossy()])?;
        let (articles, projects) = counts(&connection)?;
        drop(connection);

        let mut sources = vec![(DATABASE_FILE.to_owned(), snapshot.to_path_buf())];
        let library = self.storage_dir.join("library");
        if library.is_dir() {
            let mut files = Vec::new();
            walk(&library, &mut files)?;
            files.sort();
            for file in files {
                let relative = file
                    .strip_prefix(&self.storage_dir)
                    .map_err(|_| AppError::Initialization("Ruta de biblioteca inválida".into()))?;
                let name = relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                sources.push((name, file));
            }
        }

        let mut zip = ZipWriter::new(BufWriter::new(File::create(partial)?));
        let mut entries = Vec::with_capacity(sources.len());
        let mut buffer = vec![0_u8; 256 * 1024];
        for (name, path) in &sources {
            let bytes = fs::metadata(path)?.len();
            let method = if name == DATABASE_FILE {
                CompressionMethod::Deflated
            } else {
                // PDFs are already compressed.
                CompressionMethod::Stored
            };
            let options = SimpleFileOptions::default()
                .compression_method(method)
                .large_file(bytes > u32::MAX as u64);
            zip.start_file(name.as_str(), options).map_err(zip_error)?;
            let mut reader = BufReader::new(File::open(path)?);
            let mut hasher = Sha256::new();
            loop {
                let read = reader.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                zip.write_all(&buffer[..read])?;
            }
            entries.push(BackupEntry {
                path: name.clone(),
                sha256: hex(hasher),
                bytes,
            });
        }
        let created_at = now();
        let manifest = BackupManifest {
            format: BACKUP_FORMAT.into(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: SCHEMA_VERSION,
            created_at: created_at.clone(),
            storage_dir: self.storage_dir.to_string_lossy().into_owned(),
            articles,
            projects,
            files: entries,
        };
        zip.start_file(MANIFEST_ENTRY, SimpleFileOptions::default())
            .map_err(zip_error)?;
        zip.write_all(&serde_json::to_vec_pretty(&manifest).map_err(json_error)?)?;
        zip.finish().map_err(zip_error)?.flush()?;
        fs::rename(partial, destination)?;

        let sha256 = sha256_file(destination)?;
        let bytes = fs::metadata(destination)?.len();
        self.open()?.execute(
            "INSERT INTO audit_events (id, entity_type, entity_id, action, actor_type, actor_name, after_json, created_at)
             VALUES (?1, 'backup', ?2, 'created', 'system', 'backup', ?3, ?4)",
            params![
                Uuid::new_v4().to_string(),
                sha256,
                serde_json::json!({ "path": destination.to_string_lossy(), "files": sources.len(), "bytes": bytes }).to_string(),
                created_at,
            ],
        )?;
        Ok(BackupResult {
            path: destination.to_string_lossy().into_owned(),
            sha256,
            bytes,
            files: sources.len(),
            created_at,
        })
    }

    /// Verify every entry of a backup without changing anything.
    pub fn inspect_backup(&self, source: &Path) -> AppResult<BackupInfo> {
        let manifest = unpack(source, None)?;
        Ok(info(&manifest, None))
    }

    /// Verify and unpack a backup so it replaces the current data at the next start.
    /// The current state is saved first as a pre-restore backup.
    pub fn stage_restore(&self, source: &Path) -> AppResult<BackupInfo> {
        let source_sha256 = sha256_file(source)?;
        let pending = self.storage_dir.join(PENDING_DIR);
        if pending.exists() {
            fs::remove_dir_all(&pending)?;
        }
        fs::create_dir_all(&pending)?;
        let staged = (|| -> AppResult<BackupInfo> {
            let manifest = unpack(source, Some(&pending))?;
            let staged_db = Connection::open_with_flags(
                pending.join(DATABASE_FILE),
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            let integrity: String =
                staged_db.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
            if integrity != "ok" {
                return Err(AppError::Validation(format!(
                    "La base de datos del respaldo está dañada: {integrity}"
                )));
            }
            drop(staged_db);
            let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            let pre_restore = self
                .storage_dir
                .join("backups")
                .join(format!("pre-restore-{stamp}.zip"));
            let pre_restore = self.create_backup(&pre_restore)?.path;
            let outcome = RestoreOutcome {
                source_sha256,
                backup_created_at: manifest.created_at.clone(),
                pre_restore_backup: Some(pre_restore.clone()),
            };
            fs::write(
                pending.join(MANIFEST_ENTRY),
                serde_json::to_vec_pretty(&manifest).map_err(json_error)?,
            )?;
            fs::write(
                pending.join(READY_MARKER),
                serde_json::to_vec(&outcome).map_err(json_error)?,
            )?;
            Ok(info(&manifest, Some(pre_restore)))
        })();
        if staged.is_err() {
            let _ = fs::remove_dir_all(&pending);
        }
        staged
    }

    pub fn cancel_restore(&self) -> AppResult<()> {
        let pending = self.storage_dir.join(PENDING_DIR);
        if pending.exists() {
            fs::remove_dir_all(pending)?;
        }
        Ok(())
    }

    pub fn diagnostics(&self) -> AppResult<Diagnostics> {
        let connection = self.open()?;
        let integrity: String =
            connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        let foreign_key_violations: i64 =
            connection.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        let schema_version: i64 = connection.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?;
        let (articles, projects) = counts(&connection)?;
        let mut statement = connection.prepare(
            "SELECT title, pdf_path FROM articles WHERE deleted_at IS NULL AND pdf_path IS NOT NULL ORDER BY title",
        )?;
        let mut missing_pdfs = 0;
        let mut missing_pdf_titles = Vec::new();
        for row in statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (title, path) = row?;
            if !Path::new(&path).is_file() {
                missing_pdfs += 1;
                if missing_pdf_titles.len() < 20 {
                    missing_pdf_titles.push(title);
                }
            }
        }
        let mut files = Vec::new();
        let library = self.storage_dir.join("library");
        if library.is_dir() {
            walk(&library, &mut files)?;
        }
        let library_bytes = files
            .iter()
            .filter_map(|file| fs::metadata(file).ok())
            .map(|metadata| metadata.len())
            .sum();
        Ok(Diagnostics {
            schema_version,
            supported_schema_version: SCHEMA_VERSION,
            integrity,
            foreign_key_violations,
            database_bytes: fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0),
            library_files: files.len(),
            library_bytes,
            articles,
            projects,
            missing_pdfs,
            missing_pdf_titles,
            pending_restore: self
                .storage_dir
                .join(PENDING_DIR)
                .join(READY_MARKER)
                .is_file(),
        })
    }
}

/// Swap a staged restore into place. Runs at start-up before the database is opened;
/// on any failure the previous data is put back.
pub fn apply_pending_restore(storage_dir: &Path) -> AppResult<Option<RestoreOutcome>> {
    let pending = storage_dir.join(PENDING_DIR);
    let marker = pending.join(READY_MARKER);
    if !marker.is_file() {
        if pending.exists() {
            // Interrupted staging: never apply a half-written restore.
            fs::remove_dir_all(&pending)?;
        }
        return Ok(None);
    }
    let outcome: RestoreOutcome =
        serde_json::from_slice(&fs::read(&marker)?).map_err(json_error)?;
    let manifest: BackupManifest =
        serde_json::from_slice(&fs::read(pending.join(MANIFEST_ENTRY))?).map_err(json_error)?;

    let replaced = storage_dir.join(format!("restore-replaced-{}", Uuid::new_v4()));
    fs::create_dir_all(&replaced)?;
    let live: Vec<String> = [
        DATABASE_FILE.to_owned(),
        format!("{DATABASE_FILE}-wal"),
        format!("{DATABASE_FILE}-shm"),
        "library".to_owned(),
    ]
    .into();
    let mut moved = Vec::new();
    let swap = (|| -> AppResult<()> {
        for name in &live {
            let current = storage_dir.join(name);
            if current.exists() {
                fs::rename(&current, replaced.join(name))?;
                moved.push(name.clone());
            }
        }
        fs::rename(pending.join(DATABASE_FILE), storage_dir.join(DATABASE_FILE))?;
        let staged_library = pending.join("library");
        if staged_library.exists() {
            fs::rename(staged_library, storage_dir.join("library"))?;
        }
        rebase_paths(
            &storage_dir.join(DATABASE_FILE),
            Path::new(&manifest.storage_dir),
            storage_dir,
        )
    })();
    if let Err(error) = swap {
        let _ = fs::remove_file(storage_dir.join(DATABASE_FILE));
        let _ = fs::remove_dir_all(storage_dir.join("library"));
        for name in &moved {
            let _ = fs::rename(replaced.join(name), storage_dir.join(name));
        }
        let _ = fs::remove_dir_all(&pending);
        return Err(AppError::Initialization(format!(
            "No se pudo aplicar la restauración; se conservaron los datos anteriores: {error}"
        )));
    }
    // The pre-restore backup holds the replaced data.
    let _ = fs::remove_dir_all(&replaced);
    let _ = fs::remove_dir_all(&pending);
    Ok(Some(outcome))
}

/// Record a successful restore once the database is open and migrated.
pub fn record_restore(database: &Database, outcome: &RestoreOutcome) -> AppResult<()> {
    database.open()?.execute(
        "INSERT INTO audit_events (id, entity_type, entity_id, action, actor_type, actor_name, after_json, created_at)
         VALUES (?1, 'backup', ?2, 'restored', 'system', 'backup', ?3, ?4)",
        params![
            Uuid::new_v4().to_string(),
            outcome.source_sha256,
            serde_json::to_string(outcome).map_err(json_error)?,
            now(),
        ],
    )?;
    Ok(())
}

/// Stored PDF paths are absolute; point them at this machine's data folder.
fn rebase_paths(database: &Path, old_root: &Path, new_root: &Path) -> AppResult<()> {
    let old = format!(
        "{}{}",
        old_root.to_string_lossy(),
        std::path::MAIN_SEPARATOR
    );
    let new = format!(
        "{}{}",
        new_root.to_string_lossy(),
        std::path::MAIN_SEPARATOR
    );
    if old == new {
        return Ok(());
    }
    let connection = Connection::open(database)?;
    for (table, column) in [
        ("articles", "pdf_path"),
        ("document_files", "original_path"),
        ("evidence_spans", "source_path"),
    ] {
        connection.execute(
            &format!(
                "UPDATE {table} SET {column} = ?2 || substr({column}, length(?1) + 1)
                 WHERE substr({column}, 1, length(?1)) = ?1"
            ),
            params![old, new],
        )?;
    }
    Ok(())
}

/// Read the manifest and check every entry's size and hash; with `target`, also
/// write the entries there.
fn unpack(source: &Path, target: Option<&Path>) -> AppResult<BackupManifest> {
    let mut archive = ZipArchive::new(BufReader::new(File::open(source)?)).map_err(|_| {
        AppError::Validation("El archivo no es un respaldo de ResearchPilot".into())
    })?;
    let manifest: BackupManifest = {
        let entry = archive
            .by_name(MANIFEST_ENTRY)
            .map_err(|_| AppError::Validation("El respaldo no contiene manifiesto".into()))?;
        if entry.size() > MAX_MANIFEST_BYTES {
            return Err(AppError::Validation("Manifiesto demasiado grande".into()));
        }
        serde_json::from_reader(entry)
            .map_err(|error| AppError::Validation(format!("Manifiesto ilegible: {error}")))?
    };
    if manifest.format != BACKUP_FORMAT {
        return Err(AppError::Validation(format!(
            "Formato de respaldo no compatible: {}",
            manifest.format
        )));
    }
    if manifest.schema_version > SCHEMA_VERSION {
        return Err(AppError::Validation(format!(
            "El respaldo proviene de una versión más nueva (esquema {}); actualiza ResearchPilot antes de restaurarlo",
            manifest.schema_version
        )));
    }
    if !manifest.files.iter().any(|file| file.path == DATABASE_FILE) {
        return Err(AppError::Validation(
            "El respaldo no contiene la base de datos".into(),
        ));
    }
    let listed = archive.len();
    if listed != manifest.files.len() + 1 {
        return Err(AppError::Validation(
            "El contenido del respaldo no coincide con su manifiesto".into(),
        ));
    }
    if let Some(file) = manifest.files.iter().find(|file| !safe_entry(&file.path)) {
        return Err(AppError::Validation(format!(
            "Entrada no permitida en el respaldo: {}",
            file.path
        )));
    }
    let mut buffer = vec![0_u8; 256 * 1024];
    for file in &manifest.files {
        let mut entry = archive
            .by_name(&file.path)
            .map_err(|_| AppError::Validation(format!("Falta en el respaldo: {}", file.path)))?;
        let mut output = match target {
            Some(root) => {
                let path = root.join(&file.path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                Some(BufWriter::new(File::create(path)?))
            }
            None => None,
        };
        let mut hasher = Sha256::new();
        let mut bytes = 0_u64;
        loop {
            let read = entry.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            bytes += read as u64;
            if bytes > file.bytes {
                break;
            }
            hasher.update(&buffer[..read]);
            if let Some(output) = output.as_mut() {
                output.write_all(&buffer[..read])?;
            }
        }
        if let Some(mut output) = output {
            output.flush()?;
        }
        if bytes != file.bytes || hex(hasher) != file.sha256 {
            return Err(AppError::Validation(format!(
                "Huella incorrecta en {}; el respaldo está dañado o fue modificado",
                file.path
            )));
        }
    }
    Ok(manifest)
}

fn safe_entry(name: &str) -> bool {
    (name == DATABASE_FILE || name.starts_with("library/"))
        && !name.contains('\\')
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn info(manifest: &BackupManifest, pre_restore_backup: Option<String>) -> BackupInfo {
    BackupInfo {
        created_at: manifest.created_at.clone(),
        app_version: manifest.app_version.clone(),
        schema_version: manifest.schema_version,
        articles: manifest.articles,
        projects: manifest.projects,
        files: manifest.files.len(),
        bytes: manifest.files.iter().map(|file| file.bytes).sum(),
        pre_restore_backup,
    }
}

fn counts(connection: &Connection) -> AppResult<(i64, i64)> {
    Ok(connection.query_row(
        "SELECT (SELECT COUNT(*) FROM articles WHERE deleted_at IS NULL),
                (SELECT COUNT(*) FROM review_projects WHERE deleted_at IS NULL)",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

fn walk(directory: &Path, files: &mut Vec<PathBuf>) -> AppResult<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            walk(&entry.path(), files)?;
        } else if kind.is_file() {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn hex(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn zip_error(error: zip::result::ZipError) -> AppError {
    AppError::Initialization(format!("Archivo zip: {error}"))
}

fn json_error(error: serde_json::Error) -> AppError {
    AppError::Initialization(error.to_string())
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
