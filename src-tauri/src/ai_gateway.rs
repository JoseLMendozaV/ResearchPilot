//! AI Gateway: one interface for the local provider (Ollama) and the optional
//! external ones (OpenAI, Anthropic) through their official HTTP APIs.
//!
//! Policy for anything that leaves the machine:
//! - "solo local" blocks every external call, including connection tests;
//! - external processing must be globally allowed in Settings;
//! - each request (or batch) needs an explicit consent naming provider, model
//!   and the maximum cost the user accepted, checked again before sending;
//! - the monthly budget, when set, is enforced with the worst-case estimate;
//! - confidential articles are never included in external context;
//! - API keys come from the system keychain and are never stored or logged.

use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::AppSettings,
    ollama::{ChatRequest, ChatResponse, OllamaClient},
    secrets::{SecretStore, api_key_name},
};

pub const OPENAI_BASE_URL: &str = "https://api.openai.com";
pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2026-10-21";

pub trait ChatProvider: Send + Sync {
    fn provider(&self) -> &'static str;
    fn is_external(&self) -> bool;
    fn chat(
        &self,
        request: &ChatRequest<'_>,
        cancelled: &dyn Fn() -> bool,
    ) -> AppResult<ChatResponse>;
}

impl ChatProvider for OllamaClient {
    fn provider(&self) -> &'static str {
        "ollama"
    }

    fn is_external(&self) -> bool {
        false
    }

    fn chat(
        &self,
        request: &ChatRequest<'_>,
        cancelled: &dyn Fn() -> bool,
    ) -> AppResult<ChatResponse> {
        OllamaClient::chat(self, request, cancelled)
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

fn read_body(response: ureq::http::Response<ureq::Body>) -> AppResult<(u16, Value)> {
    let status = response.status().as_u16();
    let mut body = response.into_body();
    let value = body
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_json::<Value>()
        .unwrap_or(Value::Null);
    Ok((status, value))
}

fn network_error(provider: &str, error: ureq::Error) -> AppError {
    AppError::Ai(format!("No se pudo conectar con {provider}: {error}"))
}

// --- OpenAI ------------------------------------------------------------------------

pub struct OpenAiClient {
    base_url: String,
    api_key: String,
    max_output_tokens: i64,
    agent: ureq::Agent,
}

impl OpenAiClient {
    pub fn new(base_url: &str, api_key: String, max_output_tokens: i64, timeout: Duration) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            api_key,
            max_output_tokens,
            agent: agent(timeout),
        }
    }

    pub fn list_models(&self) -> AppResult<Vec<String>> {
        let response = self
            .agent
            .get(format!("{}/v1/models", self.base_url))
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .call()
            .map_err(|error| network_error("OpenAI", error))?;
        let (status, body) = read_body(response)?;
        if status != 200 {
            return Err(openai_error(status, &body));
        }
        let excluded = [
            "embedding",
            "whisper",
            "tts",
            "dall-e",
            "image",
            "audio",
            "realtime",
            "moderation",
            "transcribe",
            "search",
            "davinci",
            "babbage",
            "computer-use",
        ];
        let mut models: Vec<String> = body["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|model| model["id"].as_str().map(str::to_owned))
            .filter(|id| {
                ["gpt-", "o1", "o3", "o4", "o5", "chatgpt-"]
                    .iter()
                    .any(|prefix| id.starts_with(prefix))
            })
            .filter(|id| !excluded.iter().any(|word| id.contains(word)))
            .collect();
        models.sort();
        Ok(models)
    }
}

fn openai_error(status: u16, body: &Value) -> AppError {
    let message = body["error"]["message"]
        .as_str()
        .unwrap_or("error desconocido");
    let code = body["error"]["code"].as_str().unwrap_or("");
    match (status, code) {
        (401, _) => AppError::Ai("OpenAI rechazó la clave de API (401). Revísala en Configuración.".into()),
        (429, "insufficient_quota") => AppError::Ai(
            "La cuenta de API de OpenAI no tiene crédito. La facturación de la API es independiente de ChatGPT Plus/Pro.".into(),
        ),
        (429, _) => AppError::Ai(format!("OpenAI limitó la tasa de solicitudes (429): {message}")),
        (404, _) => AppError::Ai(format!("Modelo no disponible para esta cuenta de OpenAI: {message}")),
        _ => AppError::Ai(format!("OpenAI respondió {status}: {message}")),
    }
}

impl ChatProvider for OpenAiClient {
    fn provider(&self) -> &'static str {
        "openai"
    }

    fn is_external(&self) -> bool {
        true
    }

    fn chat(
        &self,
        request: &ChatRequest<'_>,
        cancelled: &dyn Fn() -> bool,
    ) -> AppResult<ChatResponse> {
        if cancelled() {
            return Err(AppError::Cancelled);
        }
        let started = std::time::Instant::now();
        let mut body = json!({
            "model": request.model,
            "messages": [
                { "role": "system", "content": request.system },
                { "role": "user", "content": request.user },
            ],
            "max_completion_tokens": self.max_output_tokens,
        });
        if let Some(schema) = request.format {
            body["response_format"] = json!({ "type": "json_schema", "json_schema": { "name": "researchpilot_output", "strict": true, "schema": schema } });
        }
        let response = self
            .agent
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .send_json(&body)
            .map_err(|error| network_error("OpenAI", error))?;
        let (status, value) = read_body(response)?;
        if status != 200 {
            return Err(openai_error(status, &value));
        }
        let message = &value["choices"][0]["message"];
        if let Some(refusal) = message["refusal"].as_str() {
            return Err(AppError::Ai(format!(
                "El modelo rechazó la solicitud: {refusal}"
            )));
        }
        if value["choices"][0]["finish_reason"].as_str() == Some("length") {
            return Err(AppError::Ai("La respuesta se truncó por el límite de tokens de salida; súbelo en Configuración.".into()));
        }
        Ok(ChatResponse {
            content: message["content"].as_str().unwrap_or("").to_owned(),
            prompt_tokens: value["usage"]["prompt_tokens"].as_i64(),
            completion_tokens: value["usage"]["completion_tokens"].as_i64(),
            duration_ms: started.elapsed().as_millis() as i64,
        })
    }
}

// --- Anthropic ----------------------------------------------------------------------

pub struct AnthropicClient {
    base_url: String,
    api_key: String,
    max_output_tokens: i64,
    agent: ureq::Agent,
}

/// Models whose refusals can be retried server-side on another model.
const FALLBACK_MODELS: [&str; 4] = [
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5-5",
];

impl AnthropicClient {
    pub fn new(base_url: &str, api_key: String, max_output_tokens: i64, timeout: Duration) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            api_key,
            max_output_tokens,
            agent: agent(timeout),
        }
    }

    pub fn list_models(&self) -> AppResult<Vec<String>> {
        let response = self
            .agent
            .get(format!("{}/v1/models?limit=100", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .call()
            .map_err(|error| network_error("Anthropic", error))?;
        let (status, body) = read_body(response)?;
        if status != 200 {
            return Err(anthropic_error(status, &body));
        }
        Ok(body["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|model| model["id"].as_str().map(str::to_owned))
            .collect())
    }

    fn send(&self, body: &Value, with_fallback: bool) -> AppResult<(u16, Value)> {
        let mut request = self
            .agent
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION);
        let mut body = body.clone();
        if with_fallback {
            request = request.header("anthropic-beta", "server-side-fallback-2026-07-01");
            body["fallbacks"] = json!("default");
        }
        let response = request
            .send_json(&body)
            .map_err(|error| network_error("Anthropic", error))?;
        read_body(response)
    }
}

fn anthropic_error(status: u16, body: &Value) -> AppError {
    let kind = body["error"]["type"].as_str().unwrap_or("");
    let message = body["error"]["message"]
        .as_str()
        .unwrap_or("error desconocido");
    match kind {
        "authentication_error" => AppError::Ai("Anthropic rechazó la clave de API (401). Revísala en Configuración.".into()),
        "billing_error" => AppError::Ai(
            "La cuenta de API de Anthropic no tiene crédito o tiene un problema de facturación. La API se factura aparte de Claude Pro/Max.".into(),
        ),
        "permission_error" => AppError::Ai(format!("La clave no tiene permiso para esta operación: {message}")),
        "not_found_error" => AppError::Ai(format!("Modelo no disponible para esta cuenta de Anthropic: {message}")),
        "rate_limit_error" => AppError::Ai("Anthropic limitó la tasa de solicitudes (429); espera e inténtalo de nuevo.".into()),
        "overloaded_error" => AppError::Ai("La API de Anthropic está sobrecargada temporalmente (529); reintenta más tarde.".into()),
        _ => AppError::Ai(format!("Anthropic respondió {status}: {message}")),
    }
}

impl ChatProvider for AnthropicClient {
    fn provider(&self) -> &'static str {
        "anthropic"
    }

    fn is_external(&self) -> bool {
        true
    }

    fn chat(
        &self,
        request: &ChatRequest<'_>,
        cancelled: &dyn Fn() -> bool,
    ) -> AppResult<ChatResponse> {
        if cancelled() {
            return Err(AppError::Cancelled);
        }
        let started = std::time::Instant::now();
        // Sampling parameters are not sent: API version 2026-10-21 rejects them.
        let mut body = json!({
            "model": request.model,
            "max_tokens": self.max_output_tokens,
            "system": request.system,
            "messages": [{ "role": "user", "content": request.user }],
        });
        if let Some(schema) = request.format {
            body["output_config"] =
                json!({ "format": { "type": "json_schema", "schema": schema } });
        }
        let use_fallback = FALLBACK_MODELS.contains(&request.model);
        let (mut status, mut value) = self.send(&body, use_fallback)?;
        if use_fallback
            && status == 400
            && value["error"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("fallback")
        {
            // Some request shapes may not accept fallbacks; retry once without them.
            (status, value) = self.send(&body, false)?;
        }
        if status != 200 {
            return Err(anthropic_error(status, &value));
        }
        match value["stop_reason"].as_str() {
            Some("refusal") => {
                let category = value["stop_details"]["category"]
                    .as_str()
                    .unwrap_or("sin categoría");
                return Err(AppError::Ai(format!(
                    "El modelo declinó la solicitud por política de seguridad ({category})."
                )));
            }
            Some("max_tokens") => {
                return Err(AppError::Ai("La respuesta se truncó por el límite de tokens de salida; súbelo en Configuración.".into()));
            }
            _ => {}
        }
        let content: String = value["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|block| block["type"] == "text")
            .filter_map(|block| block["text"].as_str())
            .collect();
        Ok(ChatResponse {
            content,
            prompt_tokens: value["usage"]["input_tokens"].as_i64(),
            completion_tokens: value["usage"]["output_tokens"].as_i64(),
            duration_ms: started.elapsed().as_millis() as i64,
        })
    }
}

// --- Policy, consent and costs -------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalConsent {
    pub provider: String,
    pub model: String,
    /// Highest cost (USD) the user accepted; `None` only when no price is configured.
    pub max_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleRef {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalEstimate {
    pub provider: String,
    pub model: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub expected_output_tokens: i64,
    pub max_output_tokens: i64,
    pub pricing_known: bool,
    pub expected_cost_usd: Option<f64>,
    pub worst_case_cost_usd: Option<f64>,
    pub month_spent_usd: f64,
    pub monthly_budget_usd: f64,
    pub within_budget: bool,
    pub articles: Vec<ArticleRef>,
    pub excluded_confidential: i64,
    pub excerpts: i64,
}

pub fn pricing(
    connection: &Connection,
    provider: &str,
    model: &str,
) -> AppResult<Option<(f64, f64)>> {
    Ok(connection
        .query_row(
            "SELECT input_usd_per_mtok, output_usd_per_mtok FROM ai_model_pricing WHERE provider = ?1 AND model = ?2",
            params![provider, model],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

pub fn cost_usd(price: Option<(f64, f64)>, input_tokens: i64, output_tokens: i64) -> Option<f64> {
    price.map(|(input, output)| {
        (input * input_tokens as f64 + output * output_tokens as f64) / 1_000_000.0
    })
}

pub fn month_spent(connection: &Connection) -> AppResult<f64> {
    Ok(connection.query_row(
        "SELECT COALESCE(SUM(cost_usd), 0) FROM ai_runs
         WHERE external_processing_authorized = 1 AND substr(created_at, 1, 7) = strftime('%Y-%m', 'now')",
        [],
        |row| row.get(0),
    )?)
}

/// Model used for a provider: explicit override, then the provider's setting.
pub fn resolve_model(
    settings: &AppSettings,
    provider: &str,
    model_override: Option<&str>,
    task: &str,
) -> AppResult<String> {
    if let Some(model) = model_override
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        return Ok(model.to_owned());
    }
    let model = match provider {
        "ollama" if task == "fields" && !settings.ollama_extraction_model.trim().is_empty() => {
            settings.ollama_extraction_model.trim()
        }
        "ollama" => settings.ollama_model.trim(),
        "openai" => settings.openai_model.trim(),
        "anthropic" => settings.anthropic_model.trim(),
        _ => return Err(AppError::Validation("Proveedor de IA no reconocido".into())),
    };
    if model.is_empty() {
        return Err(AppError::Ai(format!(
            "Elige un modelo para {provider} en Configuración → Inteligencia artificial"
        )));
    }
    Ok(model.to_owned())
}

pub fn ensure_external_allowed(settings: &AppSettings) -> AppResult<()> {
    if settings.local_only {
        return Err(AppError::Ai(
            "Modo solo local activo: ResearchPilot no envía nada fuera del equipo.".into(),
        ));
    }
    if !settings.allow_external_processing {
        return Err(AppError::Ai(
            "El procesamiento externo está desactivado en Configuración → Privacidad.".into(),
        ));
    }
    Ok(())
}

/// Build the provider client, enforcing the external-processing policy.
pub fn chat_provider(
    settings: &AppSettings,
    provider: &str,
    model: &str,
    consent: Option<&ExternalConsent>,
    secrets: &dyn SecretStore,
    endpoints: &Endpoints,
) -> AppResult<Box<dyn ChatProvider>> {
    let timeout = Duration::from_secs(settings.ai_timeout_seconds as u64);
    if provider == "ollama" {
        return Ok(Box::new(OllamaClient::from_settings(settings, timeout)?));
    }
    ensure_external_allowed(settings)?;
    if !consent.is_some_and(|consent| consent.provider == provider && consent.model == model) {
        return Err(AppError::Ai("Enviar contenido a un proveedor externo requiere tu confirmación explícita para este proveedor y modelo.".into()));
    }
    let key = secrets
        .get(api_key_name(provider)?)?
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| {
            AppError::Ai(format!(
                "No hay una clave de API de {provider} en el Llavero. Agrégala en Configuración."
            ))
        })?;
    Ok(match provider {
        "openai" => Box::new(OpenAiClient::new(
            &endpoints.openai,
            key,
            settings.external_max_output_tokens,
            timeout,
        )),
        "anthropic" => Box::new(AnthropicClient::new(
            &endpoints.anthropic,
            key,
            settings.external_max_output_tokens,
            timeout,
        )),
        _ => return Err(AppError::Validation("Proveedor de IA no reconocido".into())),
    })
}

/// Base URLs of the external APIs (overridable in tests).
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub openai: String,
    pub anthropic: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            openai: OPENAI_BASE_URL.into(),
            anthropic: ANTHROPIC_BASE_URL.into(),
        }
    }
}

impl Database {
    /// Cost estimate for `requests` calls of `input_tokens` each (worst case: every attempt retried).
    #[allow(clippy::too_many_arguments)]
    pub fn external_estimate(
        &self,
        settings: &AppSettings,
        provider: &str,
        model: &str,
        requests: i64,
        input_tokens_per_request: i64,
        retries: i64,
        articles: Vec<ArticleRef>,
        excluded_confidential: i64,
        excerpts: i64,
    ) -> AppResult<ExternalEstimate> {
        let connection = self.open()?;
        let price = pricing(&connection, provider, model)?;
        let expected_output = 600.min(settings.external_max_output_tokens);
        let input_tokens = requests * input_tokens_per_request;
        let expected = cost_usd(price, input_tokens, requests * expected_output);
        let worst = cost_usd(
            price,
            input_tokens * retries,
            requests * retries * settings.external_max_output_tokens,
        );
        let spent = month_spent(&connection)?;
        let budget = settings.external_monthly_budget_usd;
        let within_budget = budget <= 0.0 || worst.is_some_and(|worst| spent + worst <= budget);
        Ok(ExternalEstimate {
            provider: provider.to_owned(),
            model: model.to_owned(),
            requests,
            input_tokens,
            expected_output_tokens: requests * expected_output,
            max_output_tokens: requests * retries * settings.external_max_output_tokens,
            pricing_known: price.is_some(),
            expected_cost_usd: expected,
            worst_case_cost_usd: worst,
            month_spent_usd: spent,
            monthly_budget_usd: budget,
            within_budget,
            articles,
            excluded_confidential,
            excerpts,
        })
    }

    /// Re-check, right before sending, that the consent still covers the estimate.
    pub fn check_consent(
        &self,
        estimate: &ExternalEstimate,
        consent: &ExternalConsent,
    ) -> AppResult<()> {
        if estimate.monthly_budget_usd > 0.0 && !estimate.within_budget {
            return Err(AppError::Ai(format!(
                "La solicitud podría superar el presupuesto mensual externo ({:.2} USD de {:.2} USD usados).",
                estimate.month_spent_usd, estimate.monthly_budget_usd
            )));
        }
        if estimate.monthly_budget_usd > 0.0 && !estimate.pricing_known {
            return Err(AppError::Ai("Hay un presupuesto mensual pero el modelo no tiene precio configurado; agrégalo para poder controlarlo.".into()));
        }
        match (estimate.worst_case_cost_usd, consent.max_cost_usd) {
            (Some(worst), Some(accepted)) if worst > accepted * 1.05 + 0.0001 => {
                Err(AppError::Ai(format!(
                    "El coste máximo estimado ({worst:.4} USD) supera el que confirmaste ({accepted:.4} USD); vuelve a revisar la estimación."
                )))
            }
            (Some(_), None) => Err(AppError::Ai(
                "Confirma el coste máximo estimado antes de enviar.".into(),
            )),
            _ => Ok(()),
        }
    }

    pub fn set_model_pricing(
        &self,
        provider: &str,
        model: &str,
        input: f64,
        output: f64,
    ) -> AppResult<()> {
        if !["openai", "anthropic"].contains(&provider)
            || model.trim().is_empty()
            || input < 0.0
            || output < 0.0
            || !input.is_finite()
            || !output.is_finite()
        {
            return Err(AppError::Validation("Precio no válido".into()));
        }
        self.open()?.execute(
            "INSERT INTO ai_model_pricing (provider, model, input_usd_per_mtok, output_usd_per_mtok, source, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'user', ?5)
             ON CONFLICT(provider, model) DO UPDATE SET input_usd_per_mtok = ?3, output_usd_per_mtok = ?4, source = 'user', updated_at = ?5",
            params![provider, model.trim(), input, output, crate::db::now()],
        )?;
        Ok(())
    }

    pub fn model_pricing(&self) -> AppResult<Vec<ModelPrice>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT provider, model, input_usd_per_mtok, output_usd_per_mtok, source, updated_at FROM ai_model_pricing ORDER BY provider, model",
        )?;
        Ok(statement
            .query_map([], |row| {
                Ok(ModelPrice {
                    provider: row.get(0)?,
                    model: row.get(1)?,
                    input_usd_per_mtok: row.get(2)?,
                    output_usd_per_mtok: row.get(3)?,
                    source: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn external_log(&self, limit: i64) -> AppResult<ExternalLog> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT r.created_at, r.provider, r.model, r.operation, r.status, r.prompt_tokens, r.completion_tokens, r.cost_usd, a.title
             FROM ai_runs r LEFT JOIN articles a ON a.id = r.article_id
             WHERE r.external_processing_authorized = 1 ORDER BY r.created_at DESC LIMIT ?1",
        )?;
        let entries = statement
            .query_map([limit.clamp(1, 1000)], |row| {
                Ok(ExternalLogEntry {
                    created_at: row.get(0)?,
                    provider: row.get(1)?,
                    model: row.get(2)?,
                    operation: row.get(3)?,
                    status: row.get(4)?,
                    prompt_tokens: row.get(5)?,
                    completion_tokens: row.get(6)?,
                    cost_usd: row.get(7)?,
                    article_title: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ExternalLog {
            total_requests: connection.query_row(
                "SELECT COUNT(*) FROM ai_runs WHERE external_processing_authorized = 1",
                [],
                |row| row.get(0),
            )?,
            month_spent_usd: month_spent(&connection)?,
            entries,
        })
    }

    pub fn set_article_confidential(&self, article_id: &str, confidential: bool) -> AppResult<()> {
        let connection = self.open()?;
        let changed = connection.execute(
            "UPDATE articles SET confidential = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![confidential, article_id],
        )?;
        if changed == 0 {
            return Err(AppError::Validation("El artículo no existe".into()));
        }
        connection.execute(
            "INSERT INTO audit_events (id, entity_type, entity_id, action, actor_type, actor_name, after_json, created_at)
             VALUES (?1, 'article', ?2, 'confidentiality_changed', 'human', 'local-user', ?3, ?4)",
            params![uuid::Uuid::new_v4().to_string(), article_id, json!({ "confidential": confidential }).to_string(), crate::db::now()],
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
    pub provider: String,
    pub model: String,
    pub input_usd_per_mtok: f64,
    pub output_usd_per_mtok: f64,
    pub source: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalLogEntry {
    pub created_at: String,
    pub provider: String,
    pub model: String,
    pub operation: String,
    pub status: String,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub article_title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalLog {
    pub total_requests: i64,
    pub month_spent_usd: f64,
    pub entries: Vec<ExternalLogEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub provider: String,
    pub has_key: bool,
    pub key_hint: Option<String>,
}

pub fn provider_status(secrets: &dyn SecretStore, provider: &str) -> AppResult<ProviderStatus> {
    let key = secrets.get(api_key_name(provider)?)?;
    Ok(ProviderStatus {
        provider: provider.to_owned(),
        has_key: key.as_ref().is_some_and(|key| !key.is_empty()),
        // Only the last four characters, so the user can tell keys apart.
        key_hint: key.filter(|key| key.chars().count() > 8).map(|key| {
            format!(
                "…{}",
                key.chars()
                    .rev()
                    .take(4)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            )
        }),
    })
}

/// Connection test: lists models with the stored key. Sends no documents.
pub fn test_connection(
    settings: &AppSettings,
    provider: &str,
    secrets: &dyn SecretStore,
    endpoints: &Endpoints,
) -> AppResult<Vec<String>> {
    ensure_external_allowed(settings)?;
    let key = secrets.get(api_key_name(provider)?)?.ok_or_else(|| {
        AppError::Ai(format!(
            "No hay una clave de API de {provider} en el Llavero."
        ))
    })?;
    let timeout = Duration::from_secs(30);
    match provider {
        "openai" => OpenAiClient::new(&endpoints.openai, key, 16, timeout).list_models(),
        "anthropic" => AnthropicClient::new(&endpoints.anthropic, key, 16, timeout).list_models(),
        _ => Err(AppError::Validation(
            "Proveedor externo no reconocido".into(),
        )),
    }
}
