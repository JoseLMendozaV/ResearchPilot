mod bibliography;
mod db;
mod error;
mod extraction;
mod jobs;
mod library;
mod models;
mod worker;

use std::{path::PathBuf, sync::Arc};

use db::Database;
use error::{AppError, AppResult};
use jobs::JobRunner;
use models::{
    AddMasterValueInput, AppSettings, ArticleExtraction, BootstrapData, CreateProjectInput,
    EnqueueResult, EvidenceListItem, ExtractorStatus, ImportItemResult, JobSummary, LibraryArticle,
    LibraryCollection, MasterValue, ReviewProject, UpdateArticleMetadataInput,
    UpdateMasterValueInput, ValueRevision,
};
use tauri::{Emitter, Manager, RunEvent, State};
use worker::WorkerEnvironment;

#[tauri::command]
fn bootstrap_app(database: State<'_, Database>) -> AppResult<BootstrapData> {
    database.bootstrap()
}

#[tauri::command]
fn create_review_project(
    database: State<'_, Database>,
    input: CreateProjectInput,
) -> AppResult<ReviewProject> {
    database.create_project(input)
}

#[tauri::command]
fn save_app_settings(
    database: State<'_, Database>,
    settings: AppSettings,
) -> AppResult<AppSettings> {
    database.update_settings(settings)
}

#[tauri::command]
fn import_pdf_files(
    database: State<'_, Database>,
    runner: State<'_, JobRunner>,
    paths: Vec<String>,
) -> AppResult<Vec<ImportItemResult>> {
    let results = database.import_pdf_files(paths)?;
    if database.settings()?.auto_extract_on_import {
        let imported: Vec<String> = results
            .iter()
            .filter(|result| result.status == "imported")
            .filter_map(|result| result.article_id.clone())
            .collect();
        if !imported.is_empty() {
            database.enqueue_extractions(&imported, false)?;
            runner.wake();
        }
    }
    Ok(results)
}

#[tauri::command]
fn import_reference_files(
    database: State<'_, Database>,
    paths: Vec<String>,
) -> AppResult<Vec<ImportItemResult>> {
    database.import_reference_files(paths)
}

#[tauri::command]
fn search_library(database: State<'_, Database>, query: String) -> AppResult<Vec<LibraryArticle>> {
    database.library_articles(Some(&query))
}

#[tauri::command]
fn update_article_metadata(
    database: State<'_, Database>,
    input: UpdateArticleMetadataInput,
) -> AppResult<LibraryArticle> {
    database.update_article_metadata(input)
}

#[tauri::command]
fn archive_library_article(database: State<'_, Database>, article_id: String) -> AppResult<()> {
    database.archive_article(&article_id)
}

#[tauri::command]
fn create_library_collection(
    database: State<'_, Database>,
    name: String,
) -> AppResult<LibraryCollection> {
    database.create_library_collection(&name)
}

#[tauri::command]
fn set_article_collection(
    database: State<'_, Database>,
    article_id: String,
    collection_id: String,
    included: bool,
) -> AppResult<()> {
    database.set_article_collection(&article_id, &collection_id, included)
}

#[tauri::command]
async fn extractor_status(
    database: State<'_, Database>,
    runner: State<'_, JobRunner>,
) -> AppResult<ExtractorStatus> {
    let settings = database.settings()?;
    let environment = runner.environment().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let grobid = settings
            .grobid_enabled
            .then_some(settings.grobid_url.as_str());
        environment.status(&settings.extractor_python_path, grobid)
    })
    .await
    .map_err(|error| AppError::Worker(error.to_string()))
}

#[tauri::command]
fn prepare_extractor_environment(
    database: State<'_, Database>,
    runner: State<'_, JobRunner>,
) -> AppResult<JobSummary> {
    let job = database.enqueue_setup_job()?;
    runner.wake();
    Ok(job)
}

#[tauri::command]
fn enqueue_extraction(
    database: State<'_, Database>,
    runner: State<'_, JobRunner>,
    article_ids: Vec<String>,
    force: bool,
) -> AppResult<Vec<EnqueueResult>> {
    let results = database.enqueue_extractions(&article_ids, force)?;
    runner.wake();
    Ok(results)
}

#[tauri::command]
fn enqueue_pending_extractions(
    database: State<'_, Database>,
    runner: State<'_, JobRunner>,
) -> AppResult<Vec<EnqueueResult>> {
    let results = database.enqueue_pending_extractions()?;
    runner.wake();
    Ok(results)
}

#[tauri::command]
fn list_jobs(database: State<'_, Database>, limit: i64) -> AppResult<Vec<JobSummary>> {
    database.list_jobs(limit)
}

#[tauri::command]
fn pause_job(runner: State<'_, JobRunner>, job_id: String) -> AppResult<JobSummary> {
    runner.pause(&job_id)
}

#[tauri::command]
fn resume_job(runner: State<'_, JobRunner>, job_id: String) -> AppResult<JobSummary> {
    runner.resume(&job_id)
}

#[tauri::command]
fn cancel_job(runner: State<'_, JobRunner>, job_id: String) -> AppResult<JobSummary> {
    runner.cancel(&job_id)
}

#[tauri::command]
fn retry_job(runner: State<'_, JobRunner>, job_id: String) -> AppResult<JobSummary> {
    runner.retry(&job_id)
}

#[tauri::command]
fn get_article_extraction(
    database: State<'_, Database>,
    article_id: String,
) -> AppResult<ArticleExtraction> {
    database.article_extraction(&article_id)
}

#[tauri::command]
fn update_master_value(
    database: State<'_, Database>,
    input: UpdateMasterValueInput,
) -> AppResult<MasterValue> {
    database.update_master_value(input)
}

#[tauri::command]
fn add_master_value(
    database: State<'_, Database>,
    input: AddMasterValueInput,
) -> AppResult<MasterValue> {
    database.add_master_value(input)
}

#[tauri::command]
fn master_value_history(
    database: State<'_, Database>,
    value_id: String,
) -> AppResult<Vec<ValueRevision>> {
    database.master_value_history(&value_id)
}

#[tauri::command]
fn revert_master_value(
    database: State<'_, Database>,
    value_id: String,
    revision_number: i64,
) -> AppResult<MasterValue> {
    database.revert_master_value(&value_id, revision_number)
}

#[tauri::command]
fn list_recent_evidence(
    database: State<'_, Database>,
    limit: i64,
) -> AppResult<Vec<EvidenceListItem>> {
    database.recent_evidence(limit)
}

fn worker_environment(
    app: &tauri::App,
    app_data_dir: &std::path::Path,
) -> AppResult<WorkerEnvironment> {
    // Debug builds run the worker straight from the repository so Python edits
    // apply without rebuilding; release builds use the bundled resources.
    let (worker_root, dev_python) = if cfg!(debug_assertions) {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("python");
        let dev_python = repository.join(".venv").join("bin").join("python3");
        (repository, Some(dev_python))
    } else {
        let resources = app
            .path()
            .resource_dir()
            .map_err(|error| AppError::Initialization(error.to_string()))?;
        (resources.join("worker"), None)
    };
    Ok(WorkerEnvironment {
        requirements: worker_root.join("requirements.txt"),
        worker_root,
        app_env_dir: app_data_dir.join("extractor-env"),
        dev_python,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| AppError::Initialization(error.to_string()))?;
            let database = Database::initialize(app_data_dir.join("researchpilot.sqlite3"))?;
            let handle = app.handle().clone();
            let runner = JobRunner::new(
                database.clone(),
                worker_environment(app, &app_data_dir)?,
                Arc::new(move |job: JobSummary| {
                    let _ = handle.emit("jobs://changed", job);
                }),
            );
            runner.start()?;
            app.manage(database);
            app.manage(runner);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap_app,
            create_review_project,
            save_app_settings,
            import_pdf_files,
            import_reference_files,
            search_library,
            update_article_metadata,
            archive_library_article,
            create_library_collection,
            set_article_collection,
            extractor_status,
            prepare_extractor_environment,
            enqueue_extraction,
            enqueue_pending_extractions,
            list_jobs,
            pause_job,
            resume_job,
            cancel_job,
            retry_job,
            get_article_extraction,
            update_master_value,
            add_master_value,
            master_value_history,
            revert_master_value,
            list_recent_evidence
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                app.state::<JobRunner>().shutdown();
            }
        });
}
