//! Persistent job queue processed by one background runner thread.
//!
//! Jobs live in `ingestion_jobs`, so the queue survives restarts: a job left
//! `running` by a crash is re-queued on start. Each job runs in a child
//! process that can be paused (killed and restarted later from the beginning),
//! cancelled, or killed by the timeout watchdog.

use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    ai_gateway::{Endpoints, ExternalConsent, chat_provider, resolve_model},
    db::{Database, now},
    error::{AppError, AppResult},
    models::{AppSettings, EnqueueResult, JobSummary},
    secrets::{KeychainStore, SecretStore},
    worker::{self, PROTOCOL_VERSION, RULESET_VERSION, WORKER_VERSION, WorkerEnvironment},
};

pub const EXTRACTION_JOB: &str = "scientific_extraction";
pub const SETUP_JOB: &str = "extractor_setup";
pub const INDEX_JOB: &str = "rag_index";
pub const AI_FIELDS_JOB: &str = "ai_field_extraction";
pub const MODEL_PULL_JOB: &str = "model_pull";
/// SQL list of the job types processed by the runner.
const RUNNER_JOB_TYPES: &str =
    "'scientific_extraction', 'extractor_setup', 'rag_index', 'ai_field_extraction', 'model_pull'";
const MAX_LINE_BYTES: u64 = 256 * 1024 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(400);

pub type JobNotifier = Arc<dyn Fn(JobSummary) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Control {
    Pause,
    Cancel,
}

struct Running {
    job_id: String,
    child: Option<Arc<Mutex<Child>>>,
    control: Option<Control>,
}

struct ChildRun {
    exit_code: Option<i32>,
    timed_out: bool,
    control: Option<Control>,
    stderr_tail: String,
}

struct ClaimedJob {
    id: String,
    job_type: String,
    article_id: Option<String>,
    attempts: i64,
    max_attempts: i64,
}

#[derive(Clone)]
pub struct JobRunner {
    inner: Arc<Inner>,
}

struct Inner {
    database: Database,
    environment: WorkerEnvironment,
    wake: Mutex<bool>,
    condvar: Condvar,
    current: Mutex<Option<Running>>,
    notifier: JobNotifier,
    shutdown: AtomicBool,
    /// Upper bound for child timeouts; only lowered by tests.
    timeout_cap: Mutex<Duration>,
    secrets: Arc<dyn SecretStore>,
    endpoints: Endpoints,
}

impl JobRunner {
    pub fn new(database: Database, environment: WorkerEnvironment, notifier: JobNotifier) -> Self {
        Self::with_gateway(
            database,
            environment,
            notifier,
            Arc::new(KeychainStore),
            Endpoints::default(),
        )
    }

    pub fn with_gateway(
        database: Database,
        environment: WorkerEnvironment,
        notifier: JobNotifier,
        secrets: Arc<dyn SecretStore>,
        endpoints: Endpoints,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                database,
                environment,
                wake: Mutex::new(false),
                condvar: Condvar::new(),
                current: Mutex::new(None),
                notifier,
                shutdown: AtomicBool::new(false),
                timeout_cap: Mutex::new(Duration::MAX),
                secrets,
                endpoints,
            }),
        }
    }

    pub fn environment(&self) -> &WorkerEnvironment {
        &self.inner.environment
    }

    /// Recover interrupted jobs and start processing in a background thread.
    pub fn start(&self) -> AppResult<()> {
        self.inner.database.recover_interrupted_jobs()?;
        let runner = self.clone();
        thread::Builder::new()
            .name("researchpilot-jobs".into())
            .spawn(move || {
                while !runner.inner.shutdown.load(Ordering::SeqCst) {
                    match runner.run_once() {
                        Ok(true) => continue,
                        Ok(false) => runner.wait_for_work(Duration::from_secs(5)),
                        Err(error) => {
                            eprintln!("job runner error: {error}");
                            runner.wait_for_work(Duration::from_secs(5));
                        }
                    }
                }
            })
            .map_err(|error| AppError::Initialization(error.to_string()))?;
        Ok(())
    }

    pub fn wake(&self) {
        let mut pending = self.inner.wake.lock().expect("wake lock");
        *pending = true;
        self.inner.condvar.notify_all();
    }

    pub fn shutdown(&self) {
        self.inner.shutdown.store(true, Ordering::SeqCst);
        if let Some(running) = self.inner.current.lock().expect("current lock").as_mut() {
            running.control = Some(Control::Pause);
            if let Some(child) = &running.child {
                worker::kill_quietly(&mut child.lock().expect("child lock"));
            }
        }
        self.wake();
    }

    fn wait_for_work(&self, timeout: Duration) {
        let pending = self.inner.wake.lock().expect("wake lock");
        let (mut pending, _) = self
            .inner
            .condvar
            .wait_timeout_while(pending, timeout, |pending| !*pending)
            .expect("wake wait");
        *pending = false;
    }

    /// Process the oldest queued job. Returns `false` when the queue is empty.
    pub fn run_once(&self) -> AppResult<bool> {
        let Some(job) = self.inner.database.claim_next_job()? else {
            return Ok(false);
        };
        *self.inner.current.lock().expect("current lock") = Some(Running {
            job_id: job.id.clone(),
            child: None,
            control: None,
        });
        self.notify(&job.id);
        let result = match job.job_type.as_str() {
            EXTRACTION_JOB => self.run_extraction(&job),
            SETUP_JOB => self.run_setup(&job),
            INDEX_JOB | AI_FIELDS_JOB | MODEL_PULL_JOB => self.run_in_process(&job),
            other => Err(AppError::Worker(format!(
                "Tipo de tarea desconocido: {other}"
            ))),
        };
        if let Err(error) = result {
            self.inner
                .database
                .finish_job(&job.id, "failed", None, Some(&error.to_string()))?;
        }
        *self.inner.current.lock().expect("current lock") = None;
        self.notify(&job.id);
        if job.job_type == EXTRACTION_JOB
            && let Some(article_id) = &job.article_id
            && self.inner.database.settings()?.auto_index_after_extraction
        {
            // Index whatever text exists, even after a failed layout extraction.
            let summary = self.inner.database.job_summary(&job.id)?;
            if matches!(summary.status.as_str(), "completed" | "failed") {
                for queued in self
                    .inner
                    .database
                    .enqueue_indexing(Some(std::slice::from_ref(article_id)))?
                {
                    if let Some(job_id) = queued.job_id {
                        self.notify(&job_id);
                    }
                }
            }
        }
        Ok(true)
    }

    fn control_requested(&self, job_id: &str) -> Option<Control> {
        self.inner
            .current
            .lock()
            .expect("current lock")
            .as_ref()
            .filter(|running| running.job_id == job_id)
            .and_then(|running| running.control)
    }

    /// Jobs that run inside this process (HTTP to Ollama, SQLite work). They
    /// check for pause/cancel between steps instead of killing a child.
    fn run_in_process(&self, job: &ClaimedJob) -> AppResult<()> {
        let database = &self.inner.database;
        let settings = database.settings()?;
        let cancelled = || self.control_requested(&job.id).is_some();
        let last_update = Mutex::new(Instant::now() - PROGRESS_INTERVAL);
        let progress = |fraction: f64, message: &str| -> AppResult<()> {
            let mut last = last_update.lock().expect("progress lock");
            if last.elapsed() >= PROGRESS_INTERVAL {
                *last = Instant::now();
                database.update_job_progress(&job.id, fraction, "running", Some(message))?;
                self.notify(&job.id);
            }
            Ok(())
        };
        let embed = |text: &str| -> Option<Vec<f32>> {
            let model = settings.ollama_embedding_model.trim();
            if model.is_empty() {
                return None;
            }
            let client =
                crate::ollama::OllamaClient::from_settings(&settings, Duration::from_secs(60))
                    .ok()?;
            client.embed(model, &[text.to_owned()]).ok()?.pop()
        };
        let article_id = job.article_id.as_deref().unwrap_or_default();
        let result = match job.job_type.as_str() {
            INDEX_JOB => database.index_article(article_id, &settings, &progress, &cancelled),
            AI_FIELDS_JOB => (|| {
                let input: Value = database
                    .open()?
                    .query_row(
                        "SELECT input_json FROM ingestion_jobs WHERE id = ?1",
                        [&job.id],
                        |row| row.get::<_, String>(0),
                    )
                    .map(|text| serde_json::from_str(&text).unwrap_or(Value::Null))?;
                let provider = input["provider"]
                    .as_str()
                    .filter(|provider| !provider.is_empty())
                    .unwrap_or("ollama");
                let model = resolve_model(&settings, provider, input["model"].as_str(), "fields")?;
                let consent: Option<ExternalConsent> =
                    serde_json::from_value(input["consent"].clone())
                        .ok()
                        .flatten();
                let client = chat_provider(
                    &settings,
                    provider,
                    &model,
                    consent.as_ref(),
                    self.inner.secrets.as_ref(),
                    &self.inner.endpoints,
                )?;
                database.run_ai_fields(
                    &job.id,
                    article_id,
                    &settings,
                    &embed,
                    client.as_ref(),
                    &model,
                    consent.as_ref(),
                    &progress,
                    &cancelled,
                )
            })(),
            _ => self.run_model_pull(job, &settings, &progress, &cancelled),
        };
        match result {
            Ok(summary) => database.finish_job(&job.id, "completed", Some(&summary), None),
            Err(AppError::Cancelled) => match self.control_requested(&job.id) {
                Some(Control::Pause) => database.pause_running_job(&job.id),
                _ => database.finish_job(
                    &job.id,
                    "cancelled",
                    Some("Cancelada por el usuario"),
                    None,
                ),
            },
            Err(error) => database.finish_job(&job.id, "failed", None, Some(&error.to_string())),
        }
    }

    fn run_model_pull(
        &self,
        job: &ClaimedJob,
        settings: &AppSettings,
        progress: &dyn Fn(f64, &str) -> AppResult<()>,
        cancelled: &dyn Fn() -> bool,
    ) -> AppResult<String> {
        let input: Value = self
            .inner
            .database
            .open()?
            .query_row(
                "SELECT input_json FROM ingestion_jobs WHERE id = ?1",
                [&job.id],
                |row| row.get::<_, String>(0),
            )
            .map(|text| serde_json::from_str(&text).unwrap_or(Value::Null))?;
        let model = input["model"].as_str().unwrap_or_default().to_owned();
        // Downloads can take a long time on slow connections.
        let client =
            crate::ollama::OllamaClient::from_settings(settings, Duration::from_secs(6 * 3600))?;
        client.pull(
            &model,
            |status| {
                let total = status["total"].as_f64().unwrap_or(0.0);
                let completed = status["completed"].as_f64().unwrap_or(0.0);
                let fraction = if total > 0.0 { completed / total } else { 0.0 };
                let label = status["status"].as_str().unwrap_or("descargando");
                progress(fraction.clamp(0.0, 0.99), &format!("{model}: {label}"))
            },
            cancelled,
        )?;
        Ok(format!("Modelo {model} descargado en Ollama"))
    }

    pub fn cancel(&self, job_id: &str) -> AppResult<JobSummary> {
        if self.control_running(job_id, Control::Cancel) {
            return self.inner.database.job_summary(job_id);
        }
        let changed = self.inner.database.transition_job(
            job_id,
            &["queued", "paused"],
            "cancelled",
            "Cancelada por el usuario",
        )?;
        if !changed {
            return Err(AppError::Validation(
                "La tarea no puede cancelarse en su estado actual".into(),
            ));
        }
        self.notify(job_id);
        self.inner.database.job_summary(job_id)
    }

    pub fn pause(&self, job_id: &str) -> AppResult<JobSummary> {
        if self.control_running(job_id, Control::Pause) {
            return self.inner.database.job_summary(job_id);
        }
        let changed = self.inner.database.transition_job(
            job_id,
            &["queued"],
            "paused",
            "Pausada por el usuario",
        )?;
        if !changed {
            return Err(AppError::Validation(
                "Solo se pueden pausar tareas en cola o en ejecución".into(),
            ));
        }
        self.notify(job_id);
        self.inner.database.job_summary(job_id)
    }

    pub fn resume(&self, job_id: &str) -> AppResult<JobSummary> {
        let changed = self.inner.database.transition_job(
            job_id,
            &["paused"],
            "queued",
            "Reanudada; en cola",
        )?;
        if !changed {
            return Err(AppError::Validation(
                "Solo se pueden reanudar tareas pausadas".into(),
            ));
        }
        self.notify(job_id);
        self.wake();
        self.inner.database.job_summary(job_id)
    }

    pub fn retry(&self, job_id: &str) -> AppResult<JobSummary> {
        let connection = self.inner.database.open()?;
        let changed = connection.execute(
            "UPDATE ingestion_jobs
             SET status = 'queued', attempts = 0, progress = 0, stage = NULL, error_message = NULL,
                 message = 'Reintento solicitado por el usuario', finished_at = NULL, updated_at = ?1
             WHERE id = ?2 AND status IN ('failed', 'cancelled')",
            params![now(), job_id],
        )?;
        if changed == 0 {
            return Err(AppError::Validation(
                "Solo se pueden reintentar tareas fallidas o canceladas".into(),
            ));
        }
        self.notify(job_id);
        self.wake();
        self.inner.database.job_summary(job_id)
    }

    fn control_running(&self, job_id: &str, control: Control) -> bool {
        let mut current = self.inner.current.lock().expect("current lock");
        let Some(running) = current.as_mut().filter(|running| running.job_id == job_id) else {
            return false;
        };
        running.control = Some(control);
        if let Some(child) = &running.child {
            let _ = child.lock().expect("child lock").kill();
        }
        true
    }

    fn notify(&self, job_id: &str) {
        if let Ok(summary) = self.inner.database.job_summary(job_id) {
            (self.inner.notifier)(summary);
        }
    }

    fn run_extraction(&self, job: &ClaimedJob) -> AppResult<()> {
        let database = &self.inner.database;
        let article_id = job
            .article_id
            .clone()
            .ok_or_else(|| AppError::Worker("La tarea no tiene artículo asociado".into()))?;
        let settings = database.settings()?;
        let context = database.extraction_context(&article_id)?;
        let python = self
            .inner
            .environment
            .resolve_python(&settings.extractor_python_path)
            .ok_or_else(|| {
                AppError::Worker(
                    "No se encontró Python 3. Revisa Configuración → Extracción científica.".into(),
                )
            })?;
        if !self.inner.environment.worker_present() {
            return Err(AppError::Worker(
                "No se encontró el paquete del extractor".into(),
            ));
        }
        let options = extraction_options(&settings);
        let run_id = database.start_extraction_run(&job.id, &article_id, &context, &options)?;
        let request = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "command": "extract",
            "pdfPath": context.path,
            "expectedSha256": context.sha256,
            "options": options,
            "fallbackPages": context.fallback_pages.iter().map(|(page, text)| json!({ "pageNumber": page, "text": text })).collect::<Vec<_>>(),
        });
        let payload =
            serde_json::to_vec(&request).map_err(|error| AppError::Worker(error.to_string()))?;

        let mut result: Option<Value> = None;
        let mut worker_error: Option<(String, String, bool)> = None;
        let mut last_update = Instant::now() - PROGRESS_INTERVAL;
        let outcome = self.run_child(
            &job.id,
            self.inner
                .environment
                .worker_command(&python.path, &["extract"]),
            Some(payload),
            Duration::from_secs(settings.extraction_timeout_seconds as u64 + 15),
            |line| {
                let event: Value = serde_json::from_str(line).map_err(|error| {
                    AppError::Worker(format!("Mensaje inválido del extractor: {error}"))
                })?;
                match event["type"].as_str() {
                    Some("hello")
                        if event["protocolVersion"].as_i64() != Some(PROTOCOL_VERSION) =>
                    {
                        return Err(AppError::Worker(
                            "Versión de protocolo incompatible con el extractor".into(),
                        ));
                    }
                    Some("progress") if last_update.elapsed() >= PROGRESS_INTERVAL => {
                        last_update = Instant::now();
                        let stage = event["stage"].as_str().unwrap_or("");
                        let current = event["current"].as_f64().unwrap_or(0.0);
                        let total = event["total"].as_f64().unwrap_or(1.0).max(1.0);
                        database.update_job_progress(
                            &job.id,
                            stage_progress(stage, current, total),
                            stage,
                            event["message"].as_str(),
                        )?;
                        self.notify(&job.id);
                    }
                    Some("result") => result = Some(event["payload"].clone()),
                    Some("error") => {
                        worker_error = Some((
                            event["code"].as_str().unwrap_or("error").to_owned(),
                            event["message"]
                                .as_str()
                                .unwrap_or("Error del extractor")
                                .to_owned(),
                            event["retryable"].as_bool().unwrap_or(false),
                        ))
                    }
                    _ => {}
                }
                Ok(())
            },
        );

        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                database.finish_extraction_run(&run_id, "failed", Some(&error.to_string()))?;
                return Err(error);
            }
        };
        match outcome.control {
            Some(Control::Cancel) => {
                database.finish_extraction_run(
                    &run_id,
                    "cancelled",
                    Some("Cancelada por el usuario"),
                )?;
                return database.finish_job(
                    &job.id,
                    "cancelled",
                    Some("Cancelada por el usuario"),
                    None,
                );
            }
            Some(Control::Pause) => {
                database.finish_extraction_run(
                    &run_id,
                    "cancelled",
                    Some("Pausada; se reiniciará al reanudar"),
                )?;
                return database.pause_running_job(&job.id);
            }
            None => {}
        }
        if outcome.timed_out {
            let message = format!(
                "Se superó el tiempo máximo de {} s",
                settings.extraction_timeout_seconds
            );
            database.finish_extraction_run(&run_id, "failed", Some(&message))?;
            return database.finish_job(&job.id, "failed", None, Some(&message));
        }
        if let Some(payload) = result {
            database.update_job_progress(
                &job.id,
                0.97,
                "persist",
                Some("Guardando páginas, bloques y evidencias"),
            )?;
            self.notify(&job.id);
            return match database.persist_extraction(&run_id, &article_id, &context, &payload) {
                Ok(summary) => database.finish_job(&job.id, "completed", Some(&summary), None),
                Err(error) => {
                    database.finish_extraction_run(&run_id, "failed", Some(&error.to_string()))?;
                    database.finish_job(&job.id, "failed", None, Some(&error.to_string()))
                }
            };
        }
        let (message, retryable) = match worker_error {
            Some((code, message, retryable)) => (format!("{message} ({code})"), retryable),
            None => (
                format!(
                    "El extractor terminó sin resultado (código {}). {}",
                    outcome
                        .exit_code
                        .map_or("desconocido".into(), |code| code.to_string()),
                    worker::tail(&outcome.stderr_tail, 300)
                ),
                true,
            ),
        };
        database.finish_extraction_run(&run_id, "failed", Some(&message))?;
        if retryable && job.attempts < job.max_attempts {
            return database.requeue_job(&job.id, &format!("Reintento automático: {message}"));
        }
        database.finish_job(&job.id, "failed", None, Some(&message))
    }

    fn run_setup(&self, job: &ClaimedJob) -> AppResult<()> {
        let database = &self.inner.database;
        let environment = &self.inner.environment;
        let settings = database.settings()?;
        let base_python = if settings.extractor_python_path.trim().is_empty() {
            worker::system_python()
        } else {
            Some(settings.extractor_python_path.trim().into())
        }
        .ok_or_else(|| AppError::Worker("No se encontró Python 3 en el sistema. Instálalo con Homebrew (`brew install python`) y reintenta.".into()))?;

        let partial = environment.app_env_dir.with_extension("partial");
        if partial.exists() {
            fs::remove_dir_all(&partial)?;
        }
        database.update_job_progress(
            &job.id,
            0.05,
            "venv",
            Some("Creando entorno virtual aislado"),
        )?;
        self.notify(&job.id);
        let mut venv = Command::new(&base_python);
        venv.args(["-m", "venv"]).arg(&partial);
        worker::isolate_environment(&mut venv);
        let outcome = self.run_child(&job.id, venv, None, Duration::from_secs(300), |_| Ok(()))?;
        if let Some(result) = self.setup_interrupted(job, &outcome, &partial)? {
            return result;
        }
        if outcome.exit_code != Some(0) {
            let _ = fs::remove_dir_all(&partial);
            return Err(AppError::Worker(format!(
                "No se pudo crear el entorno: {}",
                worker::tail(&outcome.stderr_tail, 300)
            )));
        }

        database.update_job_progress(
            &job.id,
            0.2,
            "pip",
            Some("Descargando pdfplumber desde PyPI (requiere internet una sola vez)"),
        )?;
        self.notify(&job.id);
        let mut pip = Command::new(worker::venv_python(&partial));
        pip.args([
            "-m",
            "pip",
            "install",
            "--disable-pip-version-check",
            "--no-input",
            "--progress-bar",
            "off",
            "-r",
        ])
        .arg(&environment.requirements);
        worker::isolate_environment(&mut pip);
        let mut last_update = Instant::now() - PROGRESS_INTERVAL;
        let outcome = self.run_child(&job.id, pip, None, Duration::from_secs(900), |line| {
            if last_update.elapsed() >= PROGRESS_INTERVAL && !line.trim().is_empty() {
                last_update = Instant::now();
                database.update_job_progress(
                    &job.id,
                    0.5,
                    "pip",
                    Some(&worker::tail(line, 160)),
                )?;
                self.notify(&job.id);
            }
            Ok(())
        })?;
        if let Some(result) = self.setup_interrupted(job, &outcome, &partial)? {
            return result;
        }
        if outcome.exit_code != Some(0) {
            let _ = fs::remove_dir_all(&partial);
            return Err(AppError::Worker(format!(
                "pip no pudo instalar las dependencias: {}",
                worker::tail(&outcome.stderr_tail, 400)
            )));
        }

        if environment.app_env_dir.exists() {
            fs::remove_dir_all(&environment.app_env_dir)?;
        }
        fs::rename(&partial, &environment.app_env_dir)?;
        database.update_job_progress(&job.id, 0.95, "verify", Some("Verificando el extractor"))?;
        let status = environment.status("", None);
        let ready = status
            .capabilities
            .as_ref()
            .is_some_and(|capabilities| !capabilities["pdfplumber"].is_null());
        if !ready {
            return Err(AppError::Worker(
                status
                    .message
                    .unwrap_or_else(|| "pdfplumber no quedó disponible".into()),
            ));
        }
        database.finish_job(
            &job.id,
            "completed",
            Some("Entorno del extractor listo"),
            None,
        )
    }

    fn setup_interrupted(
        &self,
        job: &ClaimedJob,
        outcome: &ChildRun,
        partial: &std::path::Path,
    ) -> AppResult<Option<AppResult<()>>> {
        let database = &self.inner.database;
        let result = match (outcome.control, outcome.timed_out) {
            (Some(Control::Cancel), _) => {
                database.finish_job(&job.id, "cancelled", Some("Cancelada por el usuario"), None)
            }
            (Some(Control::Pause), _) => database.pause_running_job(&job.id),
            (None, true) => database.finish_job(
                &job.id,
                "failed",
                None,
                Some("La preparación excedió el tiempo máximo"),
            ),
            (None, false) => return Ok(None),
        };
        let _ = fs::remove_dir_all(partial);
        Ok(Some(result))
    }

    /// Spawn a child process registered as the job's process, stream stdout
    /// lines to `on_line`, and enforce the timeout and pause/cancel requests.
    fn run_child(
        &self,
        job_id: &str,
        mut command: Command,
        stdin_payload: Option<Vec<u8>>,
        timeout: Duration,
        mut on_line: impl FnMut(&str) -> AppResult<()>,
    ) -> AppResult<ChildRun> {
        let timeout = timeout.min(*self.inner.timeout_cap.lock().expect("timeout lock"));
        command
            .stdin(if stdin_payload.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            AppError::Worker(format!("No se pudo iniciar el proceso auxiliar: {error}"))
        })?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take();
        let child = Arc::new(Mutex::new(child));
        {
            let mut current = self.inner.current.lock().expect("current lock");
            if let Some(running) = current.as_mut().filter(|running| running.job_id == job_id) {
                running.child = Some(child.clone());
                if running.control.is_some() {
                    let _ = child.lock().expect("child lock").kill();
                }
            }
        }

        let writer = stdin.zip(stdin_payload).map(|(mut stdin, payload)| {
            thread::spawn(move || {
                let _ = stdin.write_all(&payload);
            })
        });
        let stderr_reader = thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut stderr) = stderr {
                let mut buffer = [0_u8; 8192];
                while let Ok(read) = stderr.read(&mut buffer) {
                    if read == 0 {
                        break;
                    }
                    text.push_str(&String::from_utf8_lossy(&buffer[..read]));
                    if text.len() > 32 * 1024 {
                        text = worker::tail(&text, 16 * 1024);
                    }
                }
            }
            text
        });
        let finished = Arc::new(AtomicBool::new(false));
        let timed_out = Arc::new(AtomicBool::new(false));
        let watchdog = {
            let (child, finished, timed_out) = (child.clone(), finished.clone(), timed_out.clone());
            let started = Instant::now();
            thread::spawn(move || {
                while !finished.load(Ordering::SeqCst) {
                    if started.elapsed() > timeout {
                        timed_out.store(true, Ordering::SeqCst);
                        let _ = child.lock().expect("child lock").kill();
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            })
        };

        let mut reader = BufReader::new(stdout);
        let mut line_error = None;
        loop {
            let mut line = String::new();
            let read = match (&mut reader).take(MAX_LINE_BYTES).read_line(&mut line) {
                Ok(read) => read,
                Err(error) => {
                    line_error = Some(AppError::Worker(format!(
                        "Salida ilegible del proceso: {error}"
                    )));
                    break;
                }
            };
            if read == 0 {
                break;
            }
            if read as u64 >= MAX_LINE_BYTES && !line.ends_with('\n') {
                line_error = Some(AppError::Worker(
                    "La respuesta del extractor excede el tamaño permitido".into(),
                ));
                break;
            }
            if let Err(error) = on_line(line.trim_end()) {
                line_error = Some(error);
                break;
            }
        }
        if line_error.is_some() {
            let _ = child.lock().expect("child lock").kill();
        }
        let status = loop {
            if let Some(status) = child.lock().expect("child lock").try_wait()? {
                break status;
            }
            thread::sleep(Duration::from_millis(25));
        };
        finished.store(true, Ordering::SeqCst);
        let _ = watchdog.join();
        if let Some(writer) = writer {
            let _ = writer.join();
        }
        let stderr_tail = stderr_reader.join().unwrap_or_default();
        let control = self
            .inner
            .current
            .lock()
            .expect("current lock")
            .as_ref()
            .filter(|running| running.job_id == job_id)
            .and_then(|running| running.control);
        if let Some(error) = line_error.filter(|_| control.is_none()) {
            return Err(error);
        }
        Ok(ChildRun {
            exit_code: status.code(),
            timed_out: timed_out.load(Ordering::SeqCst),
            control,
            stderr_tail,
        })
    }
}

pub struct ExtractionContext {
    pub document_file_id: String,
    pub path: String,
    pub sha256: String,
    pub fallback_pages: Vec<(i64, String)>,
}

pub fn extraction_options(settings: &AppSettings) -> Value {
    json!({
        "ocr": { "enabled": settings.ocr_enabled, "languages": settings.ocr_languages, "dpi": 300 },
        "grobid": {
            "enabled": settings.grobid_enabled,
            "url": settings.grobid_url,
            "allowRemote": settings.allow_external_processing && !settings.local_only,
            "timeoutSeconds": 120,
        },
        "timeoutSeconds": settings.extraction_timeout_seconds,
        "maxPages": 2000,
    })
}

/// Hash of everything that changes the extraction output, for the incremental cache.
pub fn options_fingerprint(settings: &AppSettings) -> String {
    let key = json!({
        "workerVersion": WORKER_VERSION,
        "rulesetVersion": RULESET_VERSION,
        "ocr": settings.ocr_enabled.then(|| settings.ocr_languages.clone()),
        "grobid": settings.grobid_enabled.then(|| settings.grobid_url.clone()),
    });
    Sha256::digest(key.to_string().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn stage_progress(stage: &str, current: f64, total: f64) -> f64 {
    let fraction = (current / total).clamp(0.0, 1.0);
    match stage {
        "hash" => 0.02,
        "layout" => 0.05 + 0.6 * fraction,
        "ocr" => 0.65 + 0.15 * fraction,
        "grobid" => 0.8 + 0.05 * fraction,
        "references" => 0.86,
        "rules" => 0.9,
        "done" => 0.95,
        _ => 0.05,
    }
}

const JOB_SELECT: &str =
    "SELECT j.id, j.job_type, j.status, j.progress, j.stage, j.message, j.error_message,
        j.article_id, a.title, j.attempts, j.max_attempts, j.created_at, j.updated_at,
        j.started_at, j.finished_at
     FROM ingestion_jobs j LEFT JOIN articles a ON a.id = j.article_id";

fn map_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobSummary> {
    Ok(JobSummary {
        id: row.get(0)?,
        job_type: row.get(1)?,
        status: row.get(2)?,
        progress: row.get(3)?,
        stage: row.get(4)?,
        message: row.get(5)?,
        error_message: row.get(6)?,
        article_id: row.get(7)?,
        article_title: row.get(8)?,
        attempts: row.get(9)?,
        max_attempts: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        started_at: row.get(13)?,
        finished_at: row.get(14)?,
    })
}

impl Database {
    pub fn list_jobs(&self, limit: i64) -> AppResult<Vec<JobSummary>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(&format!(
            "{JOB_SELECT}
             WHERE j.job_type IN ({RUNNER_JOB_TYPES})
             ORDER BY CASE j.status WHEN 'running' THEN 0 WHEN 'queued' THEN 1 WHEN 'paused' THEN 2 ELSE 3 END,
                      CASE WHEN j.status IN ('queued', 'paused') THEN j.created_at END ASC,
                      j.updated_at DESC
             LIMIT ?1"
        ))?;
        Ok(statement
            .query_map([limit.clamp(1, 1000)], map_job)?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn job_summary(&self, job_id: &str) -> AppResult<JobSummary> {
        let connection = self.open()?;
        connection
            .query_row(&format!("{JOB_SELECT} WHERE j.id = ?1"), [job_id], map_job)
            .optional()?
            .ok_or_else(|| AppError::Validation("La tarea no existe".into()))
    }

    pub(crate) fn active_extraction_job(
        &self,
        connection: &Connection,
        article_id: &str,
    ) -> AppResult<Option<JobSummary>> {
        Ok(connection
            .query_row(
                &format!(
                    "{JOB_SELECT} WHERE j.article_id = ?1 AND j.job_type = '{EXTRACTION_JOB}'
                     AND j.status IN ('queued', 'running', 'paused') ORDER BY j.created_at DESC LIMIT 1"
                ),
                [article_id],
                map_job,
            )
            .optional()?)
    }

    pub fn enqueue_extractions(
        &self,
        article_ids: &[String],
        force: bool,
    ) -> AppResult<Vec<EnqueueResult>> {
        if article_ids.len() > 5000 {
            return Err(AppError::Validation(
                "Demasiados artículos en una sola solicitud".into(),
            ));
        }
        let settings = self.settings()?;
        let fingerprint = options_fingerprint(&settings);
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        let mut results = Vec::with_capacity(article_ids.len());
        for article_id in article_ids {
            let document: Option<(String, String)> = transaction
                .query_row(
                    "SELECT d.id, d.sha256 FROM articles a
                     JOIN document_files d ON d.article_id = a.id AND d.is_primary = 1 AND d.deleted_at IS NULL
                     WHERE a.id = ?1 AND a.deleted_at IS NULL ORDER BY d.imported_at DESC LIMIT 1",
                    [article_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((document_id, sha256)) = document else {
                results.push(EnqueueResult {
                    article_id: article_id.clone(),
                    status: "no_document".into(),
                    job_id: None,
                    message: Some("El artículo no tiene un PDF asociado".into()),
                });
                continue;
            };
            if let Some(job) = self.active_extraction_job(&transaction, article_id)? {
                results.push(EnqueueResult {
                    article_id: article_id.clone(),
                    status: "already_queued".into(),
                    job_id: Some(job.id),
                    message: None,
                });
                continue;
            }
            let cached: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM extraction_runs WHERE article_id = ?1 AND source_document_hash = ?2
                                AND options_sha256 = ?3 AND status = 'completed')",
                params![article_id, sha256, fingerprint],
                |row| row.get(0),
            )?;
            if cached && !force {
                results.push(EnqueueResult {
                    article_id: article_id.clone(),
                    status: "cached".into(),
                    job_id: None,
                    message: Some(
                        "Ya existe una extracción del mismo PDF con la misma configuración".into(),
                    ),
                });
                continue;
            }
            let job_id = Uuid::new_v4().to_string();
            let timestamp = now();
            transaction.execute(
                "INSERT INTO ingestion_jobs (
                    id, article_id, document_file_id, job_type, status, progress, input_json,
                    message, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, 'queued', 0, ?5, 'En cola', ?6, ?6)",
                params![
                    job_id,
                    article_id,
                    document_id,
                    EXTRACTION_JOB,
                    json!({ "force": force }).to_string(),
                    timestamp
                ],
            )?;
            results.push(EnqueueResult {
                article_id: article_id.clone(),
                status: "queued".into(),
                job_id: Some(job_id),
                message: None,
            });
        }
        transaction.commit()?;
        Ok(results)
    }

    /// Queue every active article whose current PDF has no cached extraction.
    pub fn enqueue_pending_extractions(&self) -> AppResult<Vec<EnqueueResult>> {
        let ids = {
            let connection = self.open()?;
            let mut statement = connection.prepare(
                "SELECT a.id FROM articles a WHERE a.deleted_at IS NULL AND a.pdf_path IS NOT NULL ORDER BY a.created_at",
            )?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(self
            .enqueue_extractions(&ids, false)?
            .into_iter()
            .filter(|result| result.status == "queued")
            .collect())
    }

    pub fn enqueue_model_pull(&self, model: &str) -> AppResult<JobSummary> {
        let model = model.trim();
        if model.is_empty()
            || model.len() > 200
            || !model
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.:/".contains(character))
        {
            return Err(AppError::Validation(
                "Nombre de modelo no válido (ejemplo: nombre:etiqueta)".into(),
            ));
        }
        let job_id = Uuid::new_v4().to_string();
        let timestamp = now();
        self.open()?.execute(
            "INSERT INTO ingestion_jobs (id, job_type, status, progress, input_json, message, max_attempts, created_at, updated_at)
             VALUES (?1, ?2, 'queued', 0, ?3, 'En cola', 1, ?4, ?4)",
            params![job_id, MODEL_PULL_JOB, json!({ "model": model }).to_string(), timestamp],
        )?;
        self.job_summary(&job_id)
    }

    pub fn enqueue_setup_job(&self) -> AppResult<JobSummary> {
        let connection = self.open()?;
        let active: Option<String> = connection
            .query_row(
                &format!("SELECT id FROM ingestion_jobs WHERE job_type = '{SETUP_JOB}' AND status IN ('queued', 'running', 'paused') LIMIT 1"),
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(job_id) = active {
            return self.job_summary(&job_id);
        }
        let job_id = Uuid::new_v4().to_string();
        let timestamp = now();
        connection.execute(
            "INSERT INTO ingestion_jobs (id, job_type, status, progress, input_json, message, max_attempts, created_at, updated_at)
             VALUES (?1, ?2, 'queued', 0, '{}', 'En cola', 1, ?3, ?3)",
            params![job_id, SETUP_JOB, timestamp],
        )?;
        self.job_summary(&job_id)
    }

    pub fn recover_interrupted_jobs(&self) -> AppResult<usize> {
        let connection = self.open()?;
        let timestamp = now();
        connection.execute(
            "UPDATE extraction_runs SET status = 'failed', completed_at = ?1,
                    error_message = 'Interrumpida por el cierre de la aplicación'
             WHERE status = 'running'",
            [&timestamp],
        )?;
        Ok(connection.execute(
            &format!(
                "UPDATE ingestion_jobs SET status = 'queued', progress = 0, stage = NULL, updated_at = ?1,
                        message = 'Recuperada tras un cierre; se procesará de nuevo'
                 WHERE status = 'running' AND job_type IN ({RUNNER_JOB_TYPES})"
            ),
            [&timestamp],
        )?)
    }

    fn claim_next_job(&self) -> AppResult<Option<ClaimedJob>> {
        let mut connection = self.open()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let job = transaction
            .query_row(
                &format!(
                    "SELECT id, job_type, article_id, attempts + 1, max_attempts FROM ingestion_jobs
                     WHERE status = 'queued' AND job_type IN ({RUNNER_JOB_TYPES})
                     ORDER BY created_at LIMIT 1"
                ),
                [],
                |row| {
                    Ok(ClaimedJob {
                        id: row.get(0)?,
                        job_type: row.get(1)?,
                        article_id: row.get(2)?,
                        attempts: row.get(3)?,
                        max_attempts: row.get(4)?,
                    })
                },
            )
            .optional()?;
        if let Some(job) = &job {
            transaction.execute(
                "UPDATE ingestion_jobs SET status = 'running', attempts = ?1, progress = 0, stage = 'starting',
                        message = 'Iniciando', error_message = NULL, started_at = ?2, finished_at = NULL, updated_at = ?2
                 WHERE id = ?3",
                params![job.attempts, now(), job.id],
            )?;
        }
        transaction.commit()?;
        Ok(job)
    }

    fn update_job_progress(
        &self,
        job_id: &str,
        progress: f64,
        stage: &str,
        message: Option<&str>,
    ) -> AppResult<()> {
        self.open()?.execute(
            "UPDATE ingestion_jobs SET progress = ?1, stage = ?2, message = COALESCE(?3, message), updated_at = ?4
             WHERE id = ?5 AND status = 'running'",
            params![progress.clamp(0.0, 1.0), stage, message, now(), job_id],
        )?;
        Ok(())
    }

    fn finish_job(
        &self,
        job_id: &str,
        status: &str,
        message: Option<&str>,
        error: Option<&str>,
    ) -> AppResult<()> {
        let timestamp = now();
        self.open()?.execute(
            "UPDATE ingestion_jobs SET status = ?1, message = COALESCE(?2, message), error_message = ?3,
                    progress = CASE WHEN ?1 = 'completed' THEN 1 ELSE progress END,
                    stage = CASE WHEN ?1 = 'completed' THEN 'done' ELSE stage END,
                    finished_at = ?4, updated_at = ?4
             WHERE id = ?5",
            params![status, message, error, timestamp, job_id],
        )?;
        Ok(())
    }

    fn pause_running_job(&self, job_id: &str) -> AppResult<()> {
        // A paused job restarts from the beginning, so the attempt is not counted.
        self.open()?.execute(
            "UPDATE ingestion_jobs SET status = 'paused', progress = 0, stage = NULL,
                    attempts = MAX(attempts - 1, 0), message = 'Pausada; se reiniciará desde el principio al reanudar',
                    updated_at = ?1
             WHERE id = ?2",
            params![now(), job_id],
        )?;
        Ok(())
    }

    fn requeue_job(&self, job_id: &str, message: &str) -> AppResult<()> {
        self.open()?.execute(
            "UPDATE ingestion_jobs SET status = 'queued', progress = 0, stage = NULL, message = ?1, updated_at = ?2
             WHERE id = ?3",
            params![message, now(), job_id],
        )?;
        Ok(())
    }

    fn transition_job(
        &self,
        job_id: &str,
        from: &[&str],
        to: &str,
        message: &str,
    ) -> AppResult<bool> {
        let connection = self.open()?;
        let current: Option<String> = connection
            .query_row(
                "SELECT status FROM ingestion_jobs WHERE id = ?1",
                [job_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(current) = current else {
            return Err(AppError::Validation("La tarea no existe".into()));
        };
        if !from.contains(&current.as_str()) {
            return Ok(false);
        }
        let timestamp = now();
        let finished = matches!(to, "cancelled").then_some(timestamp.clone());
        Ok(connection.execute(
            "UPDATE ingestion_jobs SET status = ?1, message = ?2, updated_at = ?3, finished_at = COALESCE(?4, finished_at)
             WHERE id = ?5 AND status = ?6",
            params![to, message, timestamp, finished, job_id, current],
        )? > 0)
    }

    pub(crate) fn extraction_context(&self, article_id: &str) -> AppResult<ExtractionContext> {
        let connection = self.open()?;
        let (document_file_id, path, sha256): (String, String, String) = connection
            .query_row(
                "SELECT d.id, d.original_path, d.sha256 FROM articles a
                 JOIN document_files d ON d.article_id = a.id AND d.is_primary = 1 AND d.deleted_at IS NULL
                 WHERE a.id = ?1 AND a.deleted_at IS NULL ORDER BY d.imported_at DESC LIMIT 1",
                [article_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::Worker("El artículo no tiene un PDF activo".into()))?;
        let mut statement = connection.prepare(
            "SELECT page_number, text_content FROM document_pages WHERE document_file_id = ?1 ORDER BY page_number",
        )?;
        let fallback_pages = statement
            .query_map([&document_file_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ExtractionContext {
            document_file_id,
            path,
            sha256,
            fallback_pages,
        })
    }

    fn start_extraction_run(
        &self,
        job_id: &str,
        article_id: &str,
        context: &ExtractionContext,
        options: &Value,
    ) -> AppResult<String> {
        let settings = self.settings()?;
        let run_id = Uuid::new_v4().to_string();
        self.open()?.execute(
            "INSERT INTO extraction_runs (
                id, article_id, provider, model, prompt_version, source_document_hash, status, started_at,
                job_id, document_file_id, extractor_version, options_json, options_sha256
             ) VALUES (?1, ?2, 'researchpilot-extractor', NULL, ?3, ?4, 'running', ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                run_id,
                article_id,
                RULESET_VERSION,
                context.sha256,
                now(),
                job_id,
                context.document_file_id,
                WORKER_VERSION,
                options.to_string(),
                options_fingerprint(&settings),
            ],
        )?;
        Ok(run_id)
    }

    pub(crate) fn finish_extraction_run(
        &self,
        run_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> AppResult<()> {
        self.open()?.execute(
            "UPDATE extraction_runs SET status = ?1, error_message = ?2, completed_at = ?3 WHERE id = ?4 AND status = 'running'",
            params![status, error, now(), run_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
