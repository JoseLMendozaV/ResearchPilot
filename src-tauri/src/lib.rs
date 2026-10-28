mod ai_fields;
mod ai_gateway;
mod bibliography;
mod db;
mod error;
mod extraction;
mod indexing;
mod jobs;
mod library;
mod matrix;
mod models;
mod ollama;
mod quality;
mod rag;
mod review;
mod screening;
mod secrets;
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

use ai_gateway::{
    Endpoints, ExternalConsent, ExternalEstimate, ExternalLog, ModelPrice, ProviderStatus,
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
use secrets::{KeychainStore, SecretStore};
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
    provider: Option<String>,
    model: Option<String>,
    consent: Option<ExternalConsent>,
) -> AppResult<JobSummary> {
    let settings = database.settings()?;
    let provider = provider.unwrap_or_else(|| "ollama".into());
    let model = ai_gateway::resolve_model(&settings, &provider, model.as_deref(), "fields")?;
    if provider != "ollama" {
        ai_gateway::ensure_external_allowed(&settings)?;
        let estimate = database.estimate_ai_fields(&article_id, &provider, &model)?;
        database.check_consent(
            &estimate,
            consent
                .as_ref()
                .ok_or_else(|| AppError::Ai("Falta la confirmación del envío externo".into()))?,
        )?;
    }
    let job = database.enqueue_ai_fields(&article_id, &provider, &model, consent.as_ref())?;
    runner.wake();
    Ok(job)
}

#[tauri::command]
fn estimate_ai_fields(
    database: State<'_, Database>,
    article_id: String,
    provider: String,
    model: Option<String>,
) -> AppResult<ExternalEstimate> {
    let settings = database.settings()?;
    ai_gateway::ensure_external_allowed(&settings)?;
    let model = ai_gateway::resolve_model(&settings, &provider, model.as_deref(), "fields")?;
    database.estimate_ai_fields(&article_id, &provider, &model)
}

#[tauri::command]
async fn estimate_assistant(
    database: State<'_, Database>,
    input: AskInput,
) -> AppResult<ExternalEstimate> {
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = database.settings()?;
        ai_gateway::ensure_external_allowed(&settings)?;
        let provider = input
            .provider
            .clone()
            .unwrap_or_else(|| settings.ai_provider.clone());
        let model =
            ai_gateway::resolve_model(&settings, &provider, input.model.as_deref(), "assistant")?;
        database.estimate_assistant(&input, &provider, &model, &|text| {
            query_embedding(&settings, text)
        })
    })
    .await
    .map_err(|error| AppError::Ai(error.to_string()))?
}

/// Keychain access shared by commands (never exposes stored values).
struct Secrets(Arc<dyn SecretStore>);

#[tauri::command]
fn provider_statuses(secrets: State<'_, Secrets>) -> AppResult<Vec<ProviderStatus>> {
    ["openai", "anthropic"]
        .iter()
        .map(|provider| ai_gateway::provider_status(secrets.0.as_ref(), provider))
        .collect()
}

#[tauri::command]
fn set_api_key(
    secrets: State<'_, Secrets>,
    provider: String,
    key: String,
) -> AppResult<ProviderStatus> {
    let key = key.trim();
    if key.len() < 20 || key.len() > 400 || key.chars().any(char::is_whitespace) {
        return Err(AppError::Validation(
            "La clave de API no tiene un formato válido".into(),
        ));
    }
    secrets.0.set(secrets::api_key_name(&provider)?, key)?;
    ai_gateway::provider_status(secrets.0.as_ref(), &provider)
}

#[tauri::command]
fn delete_api_key(secrets: State<'_, Secrets>, provider: String) -> AppResult<ProviderStatus> {
    secrets.0.delete(secrets::api_key_name(&provider)?)?;
    ai_gateway::provider_status(secrets.0.as_ref(), &provider)
}

#[tauri::command]
async fn test_provider(
    database: State<'_, Database>,
    secrets: State<'_, Secrets>,
    provider: String,
) -> AppResult<Vec<String>> {
    let settings = database.settings()?;
    let store = secrets.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ai_gateway::test_connection(&settings, &provider, store.as_ref(), &Endpoints::default())
    })
    .await
    .map_err(|error| AppError::Ai(error.to_string()))?
}

#[tauri::command]
fn list_model_pricing(database: State<'_, Database>) -> AppResult<Vec<ModelPrice>> {
    database.model_pricing()
}

#[tauri::command]
fn set_model_pricing(
    database: State<'_, Database>,
    provider: String,
    model: String,
    input_usd_per_mtok: f64,
    output_usd_per_mtok: f64,
) -> AppResult<Vec<ModelPrice>> {
    database.set_model_pricing(&provider, &model, input_usd_per_mtok, output_usd_per_mtok)?;
    database.model_pricing()
}

#[tauri::command]
fn external_log(database: State<'_, Database>, limit: i64) -> AppResult<ExternalLog> {
    database.external_log(limit)
}

#[tauri::command]
fn set_article_confidential(
    database: State<'_, Database>,
    article_id: String,
    confidential: bool,
) -> AppResult<()> {
    database.set_article_confidential(&article_id, confidential)
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
    secrets: State<'_, Secrets>,
    input: AskInput,
) -> AppResult<ConversationDetail> {
    let store = secrets.0.clone();
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
        let provider = input
            .provider
            .clone()
            .unwrap_or_else(|| settings.ai_provider.clone());
        let model =
            ai_gateway::resolve_model(&settings, &provider, input.model.as_deref(), "assistant")?;
        let client = ai_gateway::chat_provider(
            &settings,
            &provider,
            &model,
            input.consent.as_ref(),
            store.as_ref(),
            &Endpoints::default(),
        )?;
        database.ask_assistant(
            &input,
            &|text| query_embedding(&settings, text),
            client.as_ref(),
            &model,
            input.consent.as_ref(),
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

// --- Phase 6: systematic review methodology --------------------------------------------

#[tauri::command]
fn project_workspace(
    database: State<'_, Database>,
    project_id: String,
) -> AppResult<review::ProjectWorkspace> {
    database.project_workspace(&project_id)
}

#[tauri::command]
fn update_project_settings(
    database: State<'_, Database>,
    input: review::ProjectSettingsInput,
) -> AppResult<review::ProjectWorkspace> {
    database.update_project_settings(input)
}

#[tauri::command]
fn add_project_reviewer(
    database: State<'_, Database>,
    project_id: String,
    name: String,
    role: String,
) -> AppResult<review::ProjectWorkspace> {
    database.add_project_reviewer(&project_id, &name, &role)
}

#[tauri::command]
fn set_active_reviewer(
    database: State<'_, Database>,
    project_id: String,
    reviewer_id: String,
) -> AppResult<review::ProjectWorkspace> {
    database.set_active_reviewer(&project_id, &reviewer_id)
}

#[tauri::command]
fn save_protocol(
    database: State<'_, Database>,
    project_id: String,
    content: serde_json::Value,
    amendment_reason: Option<String>,
) -> AppResult<Vec<review::ProtocolVersion>> {
    database.save_protocol(&project_id, content, amendment_reason)
}

#[tauri::command]
fn approve_protocol(
    database: State<'_, Database>,
    project_id: String,
) -> AppResult<Vec<review::ProtocolVersion>> {
    database.approve_protocol(&project_id)
}

#[tauri::command]
fn add_to_corpus(
    database: State<'_, Database>,
    project_id: String,
    article_ids: Vec<String>,
) -> AppResult<i64> {
    database.add_to_corpus(&project_id, &article_ids, Some("Biblioteca"))
}

#[tauri::command]
fn set_corpus_flags(
    database: State<'_, Database>,
    project_id: String,
    article_id: String,
    is_secondary: Option<bool>,
    removed: Option<bool>,
) -> AppResult<()> {
    database.set_corpus_flags(&project_id, &article_id, is_secondary, removed, None)
}

#[tauri::command]
fn control_coverage(
    database: State<'_, Database>,
    project_id: String,
) -> AppResult<review::ControlCoverage> {
    database.control_coverage(&project_id)
}

#[tauri::command]
fn add_control_article(
    database: State<'_, Database>,
    project_id: String,
    article_id: String,
    rationale: String,
) -> AppResult<review::ControlCoverage> {
    database.add_control_article(&project_id, &article_id, &rationale)
}

#[tauri::command]
fn remove_control_article(
    database: State<'_, Database>,
    project_id: String,
    control_id: String,
) -> AppResult<review::ControlCoverage> {
    database.remove_control_article(&project_id, &control_id)
}

#[tauri::command]
fn research_questions(
    database: State<'_, Database>,
    project_id: String,
) -> AppResult<review::QuestionsOverview> {
    database.research_questions(&project_id)
}

#[tauri::command]
fn save_research_question(
    database: State<'_, Database>,
    input: review::QuestionInput,
) -> AppResult<review::QuestionsOverview> {
    database.save_research_question(input)
}

#[tauri::command]
fn delete_research_question(
    database: State<'_, Database>,
    project_id: String,
    question_id: String,
) -> AppResult<review::QuestionsOverview> {
    database.delete_research_question(&project_id, &question_id)
}

#[tauri::command]
fn save_extraction_field(
    database: State<'_, Database>,
    input: review::FieldInput,
) -> AppResult<review::QuestionsOverview> {
    database.save_extraction_field(input)
}

#[tauri::command]
fn delete_extraction_field(
    database: State<'_, Database>,
    project_id: String,
    field_id: String,
) -> AppResult<review::QuestionsOverview> {
    database.delete_extraction_field(&project_id, &field_id)
}

#[tauri::command]
fn search_strategies(
    database: State<'_, Database>,
    project_id: String,
) -> AppResult<Vec<review::SearchStrategy>> {
    database.search_strategies(&project_id)
}

#[tauri::command]
fn save_search_strategy(
    database: State<'_, Database>,
    input: review::StrategyInput,
) -> AppResult<Vec<review::SearchStrategy>> {
    database.save_search_strategy(input)
}

#[tauri::command]
fn delete_search_strategy(
    database: State<'_, Database>,
    project_id: String,
    strategy_id: String,
) -> AppResult<Vec<review::SearchStrategy>> {
    database.delete_search_strategy(&project_id, &strategy_id)
}

#[tauri::command]
fn import_strategy_results(
    database: State<'_, Database>,
    strategy_id: String,
    path: String,
) -> AppResult<review::StrategyImportResult> {
    database.import_strategy_results(&strategy_id, &path)
}

#[tauri::command]
fn eligibility_criteria(
    database: State<'_, Database>,
    project_id: String,
) -> AppResult<Vec<screening::Criterion>> {
    database.eligibility_criteria(&project_id)
}

#[tauri::command]
fn save_criterion(
    database: State<'_, Database>,
    input: screening::CriterionInput,
) -> AppResult<Vec<screening::Criterion>> {
    database.save_criterion(input)
}

#[tauri::command]
fn delete_criterion(
    database: State<'_, Database>,
    project_id: String,
    criterion_id: String,
) -> AppResult<Vec<screening::Criterion>> {
    database.delete_criterion(&project_id, &criterion_id)
}

#[tauri::command]
fn screening_table(
    database: State<'_, Database>,
    project_id: String,
    phase: String,
) -> AppResult<screening::ScreeningTable> {
    database.screening_table(&project_id, &phase)
}

#[tauri::command]
fn record_screening_decision(
    database: State<'_, Database>,
    input: screening::DecisionInput,
) -> AppResult<screening::ScreeningRow> {
    database.record_screening_decision(input)
}

#[tauri::command]
fn resolve_screening_conflict(
    database: State<'_, Database>,
    conflict_id: String,
    decision: String,
    criterion_id: Option<String>,
    reason: String,
) -> AppResult<screening::ScreeningRow> {
    database.resolve_screening_conflict(&conflict_id, &decision, criterion_id, &reason)
}

#[tauri::command]
fn review_conflicts(
    database: State<'_, Database>,
    project_id: String,
) -> AppResult<Vec<screening::ConflictInfo>> {
    database.conflicts(&project_id)
}

#[tauri::command]
fn quality_table(
    database: State<'_, Database>,
    project_id: String,
) -> AppResult<quality::QualityTable> {
    database.quality_table(&project_id)
}

#[tauri::command]
fn save_quality_criterion(
    database: State<'_, Database>,
    input: quality::QualityCriterionInput,
) -> AppResult<Vec<quality::QualityCriterion>> {
    database.save_quality_criterion(input)
}

#[tauri::command]
fn delete_quality_criterion(
    database: State<'_, Database>,
    project_id: String,
    criterion_id: String,
) -> AppResult<Vec<quality::QualityCriterion>> {
    database.delete_quality_criterion(&project_id, &criterion_id)
}

#[tauri::command]
fn save_quality_scores(
    database: State<'_, Database>,
    project_id: String,
    article_id: String,
    scores: Vec<quality::ScoreEntry>,
) -> AppResult<quality::QualityTable> {
    database.save_quality_scores(&project_id, &article_id, scores)
}

#[tauri::command]
fn resolve_quality_conflict(
    database: State<'_, Database>,
    conflict_id: String,
    scores: Vec<quality::ScoreEntry>,
    reason: String,
) -> AppResult<quality::QualityTable> {
    database.resolve_quality_conflict(&conflict_id, scores, &reason)
}

#[tauri::command]
fn extraction_matrix(
    database: State<'_, Database>,
    project_id: String,
    include_eligible: bool,
) -> AppResult<matrix::ExtractionMatrix> {
    database.extraction_matrix(&project_id, include_eligible)
}

#[tauri::command]
fn save_extraction_value(
    database: State<'_, Database>,
    input: matrix::CellInput,
) -> AppResult<Vec<matrix::CellValue>> {
    database.save_extraction_value(input)
}

#[tauri::command]
fn prefill_matrix_from_master(database: State<'_, Database>, project_id: String) -> AppResult<i64> {
    database.prefill_matrix_from_master(&project_id)
}

#[tauri::command]
fn list_master_fields(database: State<'_, Database>) -> AppResult<Vec<models::MasterField>> {
    database.master_fields()
}

#[tauri::command]
fn project_audit(
    database: State<'_, Database>,
    project_id: String,
    limit: i64,
) -> AppResult<Vec<review::AuditEntry>> {
    database.project_audit(&project_id, limit)
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
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
            app.manage(Secrets(Arc::new(KeychainStore)));
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
            delete_conversation,
            estimate_assistant,
            estimate_ai_fields,
            provider_statuses,
            set_api_key,
            delete_api_key,
            test_provider,
            list_model_pricing,
            set_model_pricing,
            external_log,
            set_article_confidential,
            project_workspace,
            update_project_settings,
            add_project_reviewer,
            set_active_reviewer,
            save_protocol,
            approve_protocol,
            add_to_corpus,
            set_corpus_flags,
            control_coverage,
            add_control_article,
            remove_control_article,
            research_questions,
            save_research_question,
            delete_research_question,
            save_extraction_field,
            delete_extraction_field,
            search_strategies,
            save_search_strategy,
            delete_search_strategy,
            import_strategy_results,
            eligibility_criteria,
            save_criterion,
            delete_criterion,
            screening_table,
            record_screening_decision,
            resolve_screening_conflict,
            review_conflicts,
            quality_table,
            save_quality_criterion,
            delete_quality_criterion,
            save_quality_scores,
            resolve_quality_conflict,
            extraction_matrix,
            save_extraction_value,
            prefill_matrix_from_master,
            project_audit,
            list_master_fields
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                app.state::<JobRunner>().shutdown();
            }
        });
}
