//! Backup, verification and staged restore on temporary data folders.

use tempfile::tempdir;

use super::*;
use crate::{library::tests::create_pdf, models::CreateProjectInput};

fn database_with_pdf(root: &Path) -> (Database, String) {
    let database = Database::initialize(root.join(DATABASE_FILE)).unwrap();
    let pdf = root.join("source.pdf");
    create_pdf(&pdf);
    let article_id = database
        .import_pdf_files(vec![pdf.to_string_lossy().into()])
        .unwrap()[0]
        .article_id
        .clone()
        .unwrap();
    database
        .create_project(CreateProjectInput {
            name: "Revisión respaldada".into(),
            description: String::new(),
            review_type: "systematic_review".into(),
        })
        .unwrap();
    (database, article_id)
}

#[test]
fn backup_contains_a_verified_database_and_library() {
    let directory = tempdir().unwrap();
    let (database, _) = database_with_pdf(directory.path());
    let target = directory.path().join("out").join("respaldo.zip");

    let result = database.create_backup(&target).unwrap();
    assert_eq!(result.files, 2, "database + one PDF");
    assert_eq!(result.sha256, sha256_file(&target).unwrap());
    assert!(!target.with_extension("zip.partial").exists());

    let info = database.inspect_backup(&target).unwrap();
    assert_eq!(info.articles, 1);
    assert_eq!(info.projects, 1);
    assert_eq!(info.schema_version, SCHEMA_VERSION);
    let audited: i64 = database
        .open()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE entity_type = 'backup' AND action = 'created'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audited, 1);
}

#[test]
fn tampered_or_foreign_archives_are_rejected() {
    let directory = tempdir().unwrap();
    let (database, _) = database_with_pdf(directory.path());
    let original = directory.path().join("respaldo.zip");
    database.create_backup(&original).unwrap();

    // Same manifest, altered PDF bytes.
    let tampered = directory.path().join("alterado.zip");
    {
        let mut source = ZipArchive::new(File::open(&original).unwrap()).unwrap();
        let mut writer = ZipWriter::new(File::create(&tampered).unwrap());
        for index in 0..source.len() {
            let mut entry = source.by_index(index).unwrap();
            let name = entry.name().unwrap().into_owned();
            let mut content = Vec::new();
            entry.read_to_end(&mut content).unwrap();
            if name.starts_with("library/") {
                let last = content.len() - 1;
                content[last] ^= 0xff;
            }
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&content).unwrap();
        }
        writer.finish().unwrap();
    }
    let error = database.inspect_backup(&tampered).unwrap_err().to_string();
    assert!(error.contains("Huella incorrecta"), "{error}");
    let error = database.stage_restore(&tampered).unwrap_err().to_string();
    assert!(error.contains("Huella incorrecta"), "{error}");
    assert!(!directory.path().join(PENDING_DIR).exists());

    // Path traversal in the manifest.
    let evil = directory.path().join("evil.zip");
    {
        let manifest = BackupManifest {
            format: BACKUP_FORMAT.into(),
            app_version: "0".into(),
            schema_version: 1,
            created_at: now(),
            storage_dir: "/x".into(),
            articles: 0,
            projects: 0,
            files: vec![
                BackupEntry {
                    path: DATABASE_FILE.into(),
                    sha256: String::new(),
                    bytes: 0,
                },
                BackupEntry {
                    path: "library/../../escape.pdf".into(),
                    sha256: String::new(),
                    bytes: 0,
                },
            ],
        };
        let mut writer = ZipWriter::new(File::create(&evil).unwrap());
        for name in [DATABASE_FILE, "library/../../escape.pdf"] {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
        }
        writer
            .start_file(MANIFEST_ENTRY, SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        writer.finish().unwrap();
    }
    let error = database.inspect_backup(&evil).unwrap_err().to_string();
    assert!(error.contains("no permitida"), "{error}");

    // A zip that is not a backup.
    let foreign = directory.path().join("otro.zip");
    {
        let mut writer = ZipWriter::new(File::create(&foreign).unwrap());
        writer
            .start_file("hola.txt", SimpleFileOptions::default())
            .unwrap();
        writer.finish().unwrap();
    }
    assert!(database.inspect_backup(&foreign).is_err());
}

#[test]
fn restore_is_applied_at_start_on_another_machine_and_keeps_a_safety_copy() {
    let first = tempdir().unwrap();
    let (database, article_id) = database_with_pdf(first.path());
    let archive = first.path().join("respaldo.zip");
    database.create_backup(&archive).unwrap();

    // A different data folder (another computer) with its own, unrelated data.
    let second = tempdir().unwrap();
    let other = Database::initialize(second.path().join(DATABASE_FILE)).unwrap();
    other
        .create_project(CreateProjectInput {
            name: "Datos que se reemplazan".into(),
            description: String::new(),
            review_type: "scoping_review".into(),
        })
        .unwrap();

    let staged = other.stage_restore(&archive).unwrap();
    assert_eq!(staged.articles, 1);
    let safety = PathBuf::from(staged.pre_restore_backup.clone().unwrap());
    assert!(safety.is_file());
    assert!(other.diagnostics().unwrap().pending_restore);
    // Staging alone never changes the live data.
    assert_eq!(other.projects().unwrap()[0].name, "Datos que se reemplazan");
    drop(other);

    let outcome = apply_pending_restore(second.path()).unwrap().unwrap();
    assert_eq!(outcome.source_sha256, sha256_file(&archive).unwrap());
    let restored = Database::initialize(second.path().join(DATABASE_FILE)).unwrap();
    record_restore(&restored, &outcome).unwrap();

    assert_eq!(restored.projects().unwrap()[0].name, "Revisión respaldada");
    let article = restored.article_by_id(&article_id).unwrap();
    let pdf = PathBuf::from(article.document_path.unwrap());
    assert!(
        pdf.starts_with(second.path()),
        "path rebased: {}",
        pdf.display()
    );
    assert!(pdf.is_file());
    let diagnostics = restored.diagnostics().unwrap();
    assert_eq!(diagnostics.integrity, "ok");
    assert_eq!(diagnostics.missing_pdfs, 0);
    assert!(!diagnostics.pending_restore);
    assert!(safety.is_file(), "pre-restore backup survives the swap");

    // Nothing pending: start-up is a no-op.
    assert!(apply_pending_restore(second.path()).unwrap().is_none());
}

#[test]
fn interrupted_staging_is_discarded_and_cancel_clears_a_pending_restore() {
    let directory = tempdir().unwrap();
    let (database, _) = database_with_pdf(directory.path());
    let pending = directory.path().join(PENDING_DIR);
    fs::create_dir_all(&pending).unwrap();
    fs::write(pending.join(DATABASE_FILE), b"half").unwrap();
    assert!(apply_pending_restore(directory.path()).unwrap().is_none());
    assert!(!pending.exists());

    let archive = directory.path().join("respaldo.zip");
    database.create_backup(&archive).unwrap();
    database.stage_restore(&archive).unwrap();
    database.cancel_restore().unwrap();
    assert!(!database.diagnostics().unwrap().pending_restore);
}

#[test]
fn backups_from_a_newer_schema_are_refused() {
    let directory = tempdir().unwrap();
    let (database, _) = database_with_pdf(directory.path());
    let path = directory.path().join("futuro.zip");
    let manifest = BackupManifest {
        format: BACKUP_FORMAT.into(),
        app_version: "99.0.0".into(),
        schema_version: SCHEMA_VERSION + 1,
        created_at: now(),
        storage_dir: "/x".into(),
        articles: 0,
        projects: 0,
        files: Vec::new(),
    };
    let mut writer = ZipWriter::new(File::create(&path).unwrap());
    writer
        .start_file(MANIFEST_ENTRY, SimpleFileOptions::default())
        .unwrap();
    writer
        .write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    writer.finish().unwrap();
    let error = database.inspect_backup(&path).unwrap_err().to_string();
    assert!(error.contains("más nueva"), "{error}");
}
