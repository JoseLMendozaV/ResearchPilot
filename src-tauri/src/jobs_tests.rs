use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command as StdCommand,
};

use tempfile::{TempDir, tempdir};

use super::*;
use crate::{
    library::tests::create_pdf,
    models::{AddMasterValueInput, UpdateMasterValueInput},
};

struct Fixture {
    directory: TempDir,
    database: Database,
    runner: JobRunner,
    article_id: String,
    sha256: String,
}

fn environment(root: &Path, dev_python: Option<PathBuf>) -> WorkerEnvironment {
    WorkerEnvironment {
        worker_root: root.to_path_buf(),
        requirements: root.join("requirements.txt"),
        app_env_dir: root.join("no-app-env"),
        dev_python,
    }
}

/// Library with one imported PDF and a runner whose "python" is `script`.
fn fixture() -> Fixture {
    let directory = tempdir().expect("temp directory");
    let database =
        Database::initialize(directory.path().join("researchpilot.sqlite3")).expect("database");
    let pdf = directory.path().join("paper.pdf");
    create_pdf(&pdf);
    let imported = database
        .import_pdf_files(vec![pdf.to_string_lossy().into_owned()])
        .expect("import");
    let article_id = imported[0].article_id.clone().expect("article id");
    let sha256 = database
        .article_by_id(&article_id)
        .expect("article")
        .document_sha256
        .expect("sha");

    let worker_root = directory.path().join("worker");
    fs::create_dir_all(worker_root.join("researchpilot_worker")).expect("worker dir");
    fs::write(
        worker_root.join("researchpilot_worker").join("__main__.py"),
        "",
    )
    .expect("marker");
    let runner = JobRunner::new(
        database.clone(),
        environment(&worker_root, None),
        Arc::new(|_| {}),
    );
    Fixture {
        directory,
        database,
        runner,
        article_id,
        sha256,
    }
}

impl Fixture {
    /// Install a fake worker that ignores its arguments and runs `body` after draining stdin.
    fn fake_worker(&self, body: &str) {
        let script = self.directory.path().join("fake-python.sh");
        fs::write(&script, format!("#!/bin/sh\ncat >/dev/null\n{body}\n")).expect("script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
        let mut settings = self.database.settings().expect("settings");
        settings.extractor_python_path = script.to_string_lossy().into_owned();
        self.database
            .update_settings(settings)
            .expect("settings saved");
    }

    fn fake_result(&self, payload: Value) {
        let lines = [
            json!({ "type": "hello", "protocolVersion": PROTOCOL_VERSION }),
            json!({ "type": "progress", "stage": "layout", "current": 1, "total": 1, "message": "Página 1" }),
            json!({ "type": "result", "payload": payload }),
        ];
        let output = self.directory.path().join("output.jsonl");
        fs::write(
            &output,
            lines
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .expect("output");
        self.fake_worker(&format!("cat '{}'", output.display()));
    }

    fn enqueue(&self, force: bool) -> EnqueueResult {
        self.database
            .enqueue_extractions(std::slice::from_ref(&self.article_id), force)
            .expect("enqueue")
            .remove(0)
    }

    fn job(&self, job_id: &str) -> JobSummary {
        self.database.job_summary(job_id).expect("job")
    }
}

fn payload(sha256: &str, dois: &[&str]) -> Value {
    let candidates: Vec<Value> = dois
        .iter()
        .map(|doi| {
            json!({
                "fieldKey": "bibliography.doi", "value": doi, "displayValue": doi, "valueType": "text",
                "method": "rules:doi-regex", "origin": "rules", "needsReview": false, "notes": "",
                "evidence": [{ "quote": format!("https://doi.org/{doi}"), "page": 1, "bbox": [72.0, 116.0, 240.0, 125.0], "section": null }]
            })
        })
        .chain([json!({
            "fieldKey": "evaluation_results.metrics", "value": { "metric": "accuracy", "value": 93.2, "unit": "%" },
            "displayValue": "accuracy = 93.2 %", "valueType": "metric", "unit": "%", "method": "rules:metric-pattern",
            "origin": "rules", "needsReview": false, "notes": "",
            "evidence": [{ "quote": "an accuracy of 93.2%", "page": 1, "bbox": [72.0, 183.0, 500.0, 193.0], "section": "Abstract" }]
        }), json!({ "fieldKey": "unknown.field", "value": "x", "evidence": [] })])
        .collect();
    json!({
        "extractorVersion": WORKER_VERSION, "rulesetVersion": RULESET_VERSION, "engine": "pdfplumber",
        "grobidUsed": false, "documentSha256": sha256, "pageCount": 1, "processedPages": 1,
        "pages": [{ "pageNumber": 1, "width": 595.0, "height": 842.0, "text": "https://doi.org/10.1234/a an accuracy of 93.2%",
                    "textSource": "pdf_text", "status": "extracted", "ocrConfidence": null, "columns": 1 }],
        "blocks": [{ "pageNumber": 1, "readingOrder": 0, "blockType": "heading", "text": "1 Introduction",
                     "bbox": [72.0, 70.0, 200.0, 82.0], "fontSize": 12.0, "sectionPath": "Introduction", "canonicalSection": "introduction" }],
        "sections": [],
        "tablesFigures": [{ "objectType": "table", "label": "Table 1", "caption": "Table 1: Results.", "pageNumber": 1,
                            "bbox": null, "captionBbox": [72.0, 300.0, 200.0, 310.0], "content": { "rows": [["a", "b"]] } }],
        "references": [{ "ordinal": 1, "raw": "[1] A. Author, Evidence, 2020. doi:10.5555/x", "doi": "10.5555/x", "page": 1 }],
        "candidates": candidates,
        "notFound": ["ethics_reproducibility.ethics_approval", "bibliography.doi"],
        "warnings": [{ "code": "tables_approximate", "message": "Revisa las tablas" }],
        "stats": { "blocks": 1 }
    })
}

#[test]
fn completed_extraction_persists_proposals_with_evidence_and_not_found() {
    let fixture = fixture();
    fixture.fake_result(payload(&fixture.sha256, &["10.1234/a"]));
    let queued = fixture.enqueue(false);
    assert_eq!(queued.status, "queued");
    assert!(fixture.runner.run_once().expect("run"));

    let job = fixture.job(queued.job_id.as_deref().expect("job id"));
    assert_eq!(job.status, "completed", "{job:?}");
    assert_eq!(job.progress, 1.0);

    let extraction = fixture
        .database
        .article_extraction(&fixture.article_id)
        .expect("extraction");
    let doi = extraction
        .values
        .iter()
        .find(|value| value.field_key == "bibliography.doi")
        .expect("doi");
    assert_eq!(
        (doi.status.as_str(), doi.origin.as_str()),
        ("proposed", "rules")
    );
    assert_eq!(doi.evidence[0].page, Some(1));
    assert_eq!(
        doi.evidence[0].bbox.as_deref(),
        Some(&[72.0, 116.0, 240.0, 125.0][..])
    );
    assert_eq!(doi.evidence[0].page_width, Some(595.0));
    assert_eq!(doi.prompt_version.as_deref(), Some(RULESET_VERSION));

    let not_found: Vec<_> = extraction
        .values
        .iter()
        .filter(|value| value.status == "not_found")
        .map(|value| value.field_key.as_str())
        .collect();
    assert_eq!(
        not_found,
        vec!["ethics_reproducibility.ethics_approval"],
        "a field with a candidate is never marked not_found"
    );
    assert!(
        !extraction
            .values
            .iter()
            .any(|value| value.field_key == "unknown.field")
    );
    assert_eq!(extraction.references[0].doi.as_deref(), Some("10.5555/x"));
    assert_eq!(
        extraction.tables_figures[0].label.as_deref(),
        Some("Table 1")
    );
    assert_eq!(
        extraction.sections[0].canonical.as_deref(),
        Some("introduction")
    );
    assert_eq!(extraction.pages.extracted, 1);
    assert_eq!(extraction.runs[0].status, "completed");
    assert_eq!(fixture.database.stats().expect("stats").evidence_spans, 2);
    assert_eq!(
        fixture
            .database
            .recent_evidence(10)
            .expect("evidence")
            .len(),
        2
    );

    assert_eq!(
        fixture.enqueue(false).status,
        "cached",
        "same PDF and options are not recomputed"
    );
}

#[test]
fn human_review_creates_revisions_and_can_revert() {
    let fixture = fixture();
    fixture.fake_result(payload(&fixture.sha256, &["10.1234/a"]));
    fixture.enqueue(false);
    fixture.runner.run_once().expect("run");
    let extraction = fixture
        .database
        .article_extraction(&fixture.article_id)
        .expect("extraction");
    let doi = extraction
        .values
        .iter()
        .find(|value| value.field_key == "bibliography.doi")
        .expect("doi")
        .clone();

    let review = |value_text: Option<&str>, status: &str| UpdateMasterValueInput {
        value_id: doi.id.clone(),
        value_text: value_text.map(str::to_owned),
        status: status.into(),
        notes: String::new(),
        reason: "prueba".into(),
    };
    let confirmed = fixture
        .database
        .update_master_value(review(None, "confirmed"))
        .expect("confirm");
    assert_eq!(
        (confirmed.status.as_str(), confirmed.revision_number),
        ("confirmed", 2)
    );
    assert_eq!(
        confirmed.reviewer_name.as_deref(),
        Some("Investigador local")
    );
    let corrected = fixture
        .database
        .update_master_value(review(Some("10.9999/fixed"), "confirmed"))
        .expect("correct");
    assert_eq!(corrected.value, json!("10.9999/fixed"));
    assert!(
        matches!(
            fixture
                .database
                .update_master_value(review(Some(""), "confirmed")),
            Err(AppError::Validation(_))
        ),
        "an empty value cannot be confirmed"
    );

    let reverted = fixture
        .database
        .revert_master_value(&doi.id, 1)
        .expect("revert");
    assert_eq!(
        (
            reverted.value.clone(),
            reverted.status.as_str(),
            reverted.revision_number
        ),
        (json!("10.1234/a"), "proposed", 4)
    );
    let history = fixture
        .database
        .master_value_history(&doi.id)
        .expect("history");
    assert_eq!(
        history
            .iter()
            .map(|revision| revision.revision_number)
            .collect::<Vec<_>>(),
        vec![4, 3, 2, 1]
    );
    assert_eq!(history[3].actor_type, "system");
    assert_eq!(history[0].reason, "Revertido a la revisión 1");

    let manual = fixture
        .database
        .add_master_value(AddMasterValueInput {
            article_id: fixture.article_id.clone(),
            field_key: "ethics_reproducibility.ethics_approval".into(),
            value_text: None,
            status: "not_reported".into(),
            quote: None,
            page: None,
            notes: "Revisado el texto completo".into(),
        })
        .expect("not reported by a human");
    assert_eq!(
        (manual.origin.as_str(), manual.status.as_str()),
        ("human", "not_reported")
    );
    let quoted = fixture
        .database
        .add_master_value(AddMasterValueInput {
            article_id: fixture.article_id.clone(),
            field_key: "design_methods.sample_size".into(),
            value_text: Some("120".into()),
            status: "confirmed".into(),
            quote: Some("an accuracy of 93.2%".into()),
            page: Some(1),
            notes: String::new(),
        })
        .expect("manual value with evidence");
    assert_eq!(quoted.value, json!(120.0));
    assert_eq!(
        quoted.evidence[0].verified,
        Some(true),
        "the quote exists in the stored page text"
    );

    let connection = fixture.database.open().expect("connection");
    let audits: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE entity_type = 'master_record_value'",
            [],
            |row| row.get(0),
        )
        .expect("audit");
    assert_eq!(audits, 5);
    let machine_not_reported = connection.execute(
        "UPDATE master_record_values SET status = 'not_reported', reviewer_id = NULL WHERE origin = 'rules'",
        [],
    );
    assert!(
        machine_not_reported.is_err(),
        "the schema forbids machine-declared not_reported"
    );
}

#[test]
fn rerun_supersedes_untouched_proposals_and_keeps_reviewed_values() {
    let fixture = fixture();
    fixture.fake_result(payload(&fixture.sha256, &["10.1234/a"]));
    fixture.enqueue(false);
    fixture.runner.run_once().expect("first run");
    let first = fixture
        .database
        .article_extraction(&fixture.article_id)
        .expect("extraction");
    let metric = first
        .values
        .iter()
        .find(|value| value.field_key == "evaluation_results.metrics")
        .expect("metric");
    fixture
        .database
        .update_master_value(UpdateMasterValueInput {
            value_id: metric.id.clone(),
            value_text: None,
            status: "confirmed".into(),
            notes: String::new(),
            reason: String::new(),
        })
        .expect("confirm metric");

    assert_eq!(
        fixture.enqueue(true).status,
        "queued",
        "force bypasses the cache"
    );
    fixture.runner.run_once().expect("second run");
    let second = fixture
        .database
        .article_extraction(&fixture.article_id)
        .expect("extraction");
    let metrics: Vec<_> = second
        .values
        .iter()
        .filter(|value| value.field_key == "evaluation_results.metrics")
        .collect();
    assert_eq!(metrics.len(), 1, "a confirmed value is not proposed again");
    assert_eq!(metrics[0].status, "confirmed");
    let dois: Vec<_> = second
        .values
        .iter()
        .filter(|value| value.field_key == "bibliography.doi")
        .collect();
    assert_eq!(dois.len(), 1);
    assert_ne!(
        dois[0].id,
        first
            .values
            .iter()
            .find(|value| value.field_key == "bibliography.doi")
            .unwrap()
            .id
    );
    assert_eq!(second.runs.len(), 2);
}

#[test]
fn competing_values_for_a_single_valued_field_require_review() {
    let fixture = fixture();
    fixture.fake_result(payload(&fixture.sha256, &["10.1234/a", "10.1234/b"]));
    fixture.enqueue(false);
    fixture.runner.run_once().expect("run");
    let extraction = fixture
        .database
        .article_extraction(&fixture.article_id)
        .expect("extraction");
    let dois: Vec<_> = extraction
        .values
        .iter()
        .filter(|value| value.field_key == "bibliography.doi")
        .collect();
    assert_eq!(dois.len(), 2);
    assert!(
        dois.iter()
            .all(|value| value.status == "needs_review" && value.conflict)
    );
}

#[test]
fn hash_mismatch_fails_without_writing_values() {
    let fixture = fixture();
    fixture.fake_result(payload(&"0".repeat(64), &["10.1234/a"]));
    let job_id = fixture.enqueue(false).job_id.expect("job");
    fixture.runner.run_once().expect("run");
    let job = fixture.job(&job_id);
    assert_eq!(job.status, "failed");
    assert!(job.error_message.unwrap_or_default().contains("huella"));
    assert!(
        fixture
            .database
            .article_extraction(&fixture.article_id)
            .expect("extraction")
            .values
            .is_empty()
    );
}

#[test]
fn crashes_are_retried_once_and_typed_errors_are_not() {
    let fixture = fixture();
    fixture.fake_worker("echo 'boom' >&2\nexit 3");
    let job_id = fixture.enqueue(false).job_id.expect("job");
    fixture.runner.run_once().expect("first attempt");
    let job = fixture.job(&job_id);
    assert_eq!((job.status.as_str(), job.attempts), ("queued", 1));
    fixture.runner.run_once().expect("second attempt");
    let job = fixture.job(&job_id);
    assert_eq!((job.status.as_str(), job.attempts), ("failed", 2));
    let error = job.error_message.unwrap_or_default();
    assert!(
        error.contains("código 3") && error.contains("boom"),
        "{error}"
    );

    fixture.runner.retry(&job_id).expect("manual retry");
    fixture.fake_worker(
        r#"echo '{"type":"error","code":"invalid_pdf","message":"PDF dañado","retryable":false}'"#,
    );
    fixture.runner.run_once().expect("typed error");
    let job = fixture.job(&job_id);
    assert_eq!(job.status, "failed");
    assert!(
        job.error_message
            .unwrap_or_default()
            .contains("invalid_pdf")
    );
}

#[test]
fn timeout_kills_the_worker() {
    let fixture = fixture();
    fixture.fake_worker("exec sleep 30");
    *fixture.runner.inner.timeout_cap.lock().unwrap() = Duration::from_millis(500);
    let job_id = fixture.enqueue(false).job_id.expect("job");
    let started = Instant::now();
    fixture.runner.run_once().expect("run");
    assert!(started.elapsed() < Duration::from_secs(10));
    let job = fixture.job(&job_id);
    assert_eq!(job.status, "failed");
    assert!(
        job.error_message
            .unwrap_or_default()
            .contains("tiempo máximo")
    );
}

fn wait_until_running(fixture: &Fixture, job_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while fixture.job(job_id).status != "running"
        || fixture
            .runner
            .inner
            .current
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(|running| running.child.is_none())
    {
        assert!(Instant::now() < deadline, "job never started");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn running_jobs_can_be_cancelled_paused_and_resumed() {
    let fixture = fixture();
    fixture.fake_worker("echo '{\"type\":\"hello\",\"protocolVersion\":1}'\nexec sleep 30");
    let job_id = fixture.enqueue(false).job_id.expect("job");

    let runner = fixture.runner.clone();
    let worker = thread::spawn(move || runner.run_once());
    wait_until_running(&fixture, &job_id);
    fixture.runner.pause(&job_id).expect("pause");
    worker.join().unwrap().expect("run");
    let job = fixture.job(&job_id);
    assert_eq!((job.status.as_str(), job.attempts), ("paused", 0));
    assert!(
        !fixture
            .runner
            .run_once()
            .expect("paused jobs are not picked")
    );

    fixture.runner.resume(&job_id).expect("resume");
    let runner = fixture.runner.clone();
    let worker = thread::spawn(move || runner.run_once());
    wait_until_running(&fixture, &job_id);
    fixture.runner.cancel(&job_id).expect("cancel");
    worker.join().unwrap().expect("run");
    assert_eq!(fixture.job(&job_id).status, "cancelled");
    let runs = fixture
        .database
        .article_extraction(&fixture.article_id)
        .expect("extraction")
        .runs;
    assert!(runs.iter().all(|run| run.status == "cancelled"));
}

#[test]
fn queued_jobs_can_be_paused_and_cancelled_without_running() {
    let fixture = fixture();
    let job_id = fixture.enqueue(false).job_id.expect("job");
    assert_eq!(fixture.enqueue(false).status, "already_queued");
    assert_eq!(
        fixture.runner.pause(&job_id).expect("pause").status,
        "paused"
    );
    assert_eq!(
        fixture.runner.cancel(&job_id).expect("cancel").status,
        "cancelled"
    );
    assert!(fixture.runner.resume(&job_id).is_err());
}

#[test]
fn interrupted_jobs_are_recovered_after_restart() {
    let fixture = fixture();
    let job_id = fixture.enqueue(false).job_id.expect("job");
    fixture
        .database
        .claim_next_job()
        .expect("claim")
        .expect("claimed");
    assert_eq!(fixture.job(&job_id).status, "running");
    // Simulate a crash: a new process opens the same database.
    let reopened = Database::initialize(&fixture.database.path).expect("reopen");
    assert_eq!(reopened.recover_interrupted_jobs().expect("recover"), 1);
    let job = reopened.job_summary(&job_id).expect("job");
    assert_eq!(job.status, "queued");
    assert!(job.message.unwrap_or_default().contains("Recuperada"));
}

/// End-to-end with the real Python worker when the development venv exists.
#[test]
fn real_worker_extracts_traceable_evidence() {
    let python_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("python");
    let python = python_dir.join(".venv").join("bin").join("python3");
    if !python.is_file() {
        eprintln!("omitida: crea python/.venv con pdfplumber para ejecutar esta prueba");
        return;
    }
    let directory = tempdir().expect("temp directory");
    let pdf = directory.path().join("scientific.pdf");
    let generated = StdCommand::new(&python)
        .current_dir(python_dir.join("tests"))
        .args([
            "-c",
            &format!(
                "from pdf_fixtures import scientific_paper; scientific_paper({:?})",
                pdf.to_string_lossy()
            ),
        ])
        .status()
        .expect("fixture generator");
    assert!(generated.success());

    let database =
        Database::initialize(directory.path().join("researchpilot.sqlite3")).expect("database");
    let mut settings = database.settings().expect("settings");
    settings.ocr_enabled = false;
    database.update_settings(settings).expect("settings");
    let article_id = database
        .import_pdf_files(vec![pdf.to_string_lossy().into_owned()])
        .expect("import")[0]
        .article_id
        .clone()
        .expect("article");
    let runner = JobRunner::new(
        database.clone(),
        environment(&python_dir, Some(python)),
        Arc::new(|_| {}),
    );
    let job_id = database
        .enqueue_extractions(std::slice::from_ref(&article_id), false)
        .expect("enqueue")[0]
        .job_id
        .clone()
        .expect("job");
    runner.run_once().expect("run");
    let job = database.job_summary(&job_id).expect("job");
    assert_eq!(job.status, "completed", "{job:?}");

    let extraction = database
        .article_extraction(&article_id)
        .expect("extraction");
    let doi = extraction
        .values
        .iter()
        .find(|value| value.field_key == "bibliography.doi")
        .expect("doi");
    assert_eq!(doi.value, json!("10.1234/rp.2026.001"));
    let connection = database.open().expect("connection");
    for value in extraction
        .values
        .iter()
        .filter(|value| value.status != "not_found")
    {
        for evidence in &value.evidence {
            let page = evidence.page.expect("page");
            let page_text: String = connection
                .query_row(
                    "SELECT p.text_content FROM document_pages p JOIN document_files d ON d.id = p.document_file_id
                     WHERE d.article_id = ?1 AND p.page_number = ?2",
                    params![article_id, page],
                    |row| row.get(0),
                )
                .expect("page text");
            assert!(
                page_text.contains(&evidence.quote),
                "{}: quote not on page {page}",
                value.field_key
            );
            assert!(
                evidence.bbox.is_some(),
                "{} lacks a bounding box",
                value.field_key
            );
        }
    }
    assert_eq!(extraction.references.len(), 3);
    assert!(
        extraction
            .sections
            .iter()
            .any(|section| section.canonical.as_deref() == Some("methods"))
    );
    let source: String = connection
        .query_row("SELECT text_source FROM document_pages p JOIN document_files d ON d.id = p.document_file_id WHERE d.article_id = ?1 AND p.page_number = 1", [&article_id], |row| row.get(0))
        .expect("text source");
    assert_eq!(source, "pdf_text");
}
