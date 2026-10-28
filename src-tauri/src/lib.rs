mod ai_fields;
mod bibliography;
mod db;
mod error;
mod extraction;
mod indexing;
mod jobs;
mod library;
mod models;
mod ollama;
mod rag;
mod worker;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use db::Database;
use error::{AppError, AppResult};
use indexing::IndexStatus;
use jobs::JobRunner;
use models::{
    AddMasterValueInput, AppSettings, ArticleExtraction, BootstrapData, CreateProjectInput,
    EnqueueResult, EvidenceListItem, ExtractorStatus, ImportItemResult, JobSummary, LibraryArticle,
    LibraryCollection, MasterValue, ReviewProject, UpdateArticleMetadataInput,
    UpdateMasterValueInput, ValueRevision,
};
use ollama::{OllamaClient, OllamaStatus};
use rag::{AskInput, ConversationDetail, ConversationSummary, SearchOutcome};
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

/// Cancellation flags for in-flight assistant questions, keyed by request id.
#[derive(Default)]
struct AssistantRequests(Mutex<HashMap<String, Arc<AtomicBool>>>);

#[tauri::command]
async fn ollama_status(database: State<'_, Database>) -> AppResult<OllamaStatus> {
    let settings = database.settings()?;
    tauri::async_runtime::spawn_blocking(move || ollama::status(&settings))
        .await
        .map_err(|error| AppError::Ai(error.to_string()))
}

#[tauri::command]
fn pull_ollama_model(
    database: State<'_, Database>,
    runner: State<'_, JobRunner>,
    model: String,
) -> AppResult<JobSummary> {
    let job = database.enqueue_model_pull(&model)?;
    runner.wake();
    Ok(job)
}

#[tauri::command]
fn index_status(database: State<'_, Database>) -> AppResult<IndexStatus> {
    database.index_status()
}

#[tauri::command]
fn enqueue_indexing(
    database: State<'_, Database>,
    runner: State<'_, JobRunner>,
    article_ids: Option<Vec<String>>,
) -> AppResult<Vec<EnqueueResult>> {
    let results = database.enqueue_indexing(article_ids.as_deref())?;
    runner.wake();
    Ok(results)
}

#[tauri::command]
fn enqueue_ai_fields(
    database: State<'_, Database>,
    runner: State<'_, JobRunner>,
    article_id: String,
) -> AppResult<JobSummary> {
    let job = database.enqueue_ai_fields(&article_id)?;
    runner.wake();
    Ok(job)
}

fn query_embedding(settings: &AppSettings, text: &str) -> Option<Vec<f32>> {
    let model = settings.ollama_embedding_model.trim();
    if model.is_empty() || settings.rag_mode == "lexical" {
        return None;
    }
    OllamaClient::from_settings(settings, Duration::from_secs(60))
        .ok()?
        .embed(model, &[text.to_owned()])
        .ok()?
        .pop()
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchInput {
    query: String,
    scope_type: String,
    scope_id: Option<String>,
    year_from: Option<i64>,
    year_to: Option<i64>,
    mode: Option<String>,
}

#[tauri::command]
async fn search_chunks(
    database: State<'_, Database>,
    input: SearchInput,
) -> AppResult<SearchOutcome> {
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut settings = database.settings()?;
        if let Some(mode) = input
            .mode
            .filter(|mode| ["hybrid", "lexical", "semantic"].contains(&mode.as_str()))
        {
            settings.rag_mode = mode;
        }
        let scope = database.resolve_scope(
            &input.scope_type,
            input.scope_id.as_deref(),
            input.year_from,
            input.year_to,
        )?;
        let vector = query_embedding(&settings, &input.query);
        database.search_chunks(
            &input.query,
            &scope,
            &settings.rag_mode,
            settings.ollama_embedding_model.trim(),
            vector.as_deref(),
            20,
        )
    })
    .await
    .map_err(|error| AppError::Ai(error.to_string()))?
}

#[tauri::command]
async fn ask_assistant(
    database: State<'_, Database>,
    requests: State<'_, AssistantRequests>,
    input: AskInput,
) -> AppResult<ConversationDetail> {
    let flag = Arc::new(AtomicBool::new(false));
    requests
        .0
        .lock()
        .expect("requests lock")
        .insert(input.request_id.clone(), flag.clone());
    let database = database.inner().clone();
    let request_id = input.request_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let settings = database.settings()?;
        let client = OllamaClient::from_settings(
            &settings,
            Duration::from_secs(settings.ai_timeout_seconds as u64),
        )?;
        database.ask_assistant(
            &input,
            &|text| query_embedding(&settings, text),
            &client,
            &|| flag.load(Ordering::SeqCst),
        )
    })
    .await
    .map_err(|error| AppError::Ai(error.to_string()))?;
    requests
        .0
        .lock()
        .expect("requests lock")
        .remove(&request_id);
    result
}

#[tauri::command]
fn cancel_assistant(requests: State<'_, AssistantRequests>, request_id: String) {
    if let Some(flag) = requests.0.lock().expect("requests lock").get(&request_id) {
        flag.store(true, Ordering::SeqCst);
    }
}

#[tauri::command]
fn list_conversations(database: State<'_, Database>) -> AppResult<Vec<ConversationSummary>> {
    database.conversations()
}

#[tauri::command]
fn get_conversation(
    database: State<'_, Database>,
    conversation_id: String,
) -> AppResult<ConversationDetail> {
    database.conversation(&conversation_id)
}

#[tauri::command]
fn delete_conversation(database: State<'_, Database>, conversation_id: String) -> AppResult<()> {
    database.delete_conversation(&conversation_id)
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
            app.manage(AssistantRequests::default());
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
            list_recent_evidence,
            ollama_status,
            pull_ollama_model,
            index_status,
            enqueue_indexing,
            enqueue_ai_fields,
            search_chunks,
            ask_assistant,
            cancel_assistant,
            list_conversations,
            get_conversation,
            delete_conversation
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                app.state::<JobRunner>().shutdown();
            }
        });
}
