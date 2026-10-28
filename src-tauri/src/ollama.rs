//! Minimal client for a local Ollama server.
//!
//! Only loopback URLs are accepted unless the privacy settings authorize
//! external processing, and proxy environment variables are ignored so a
//! local call can never be routed elsewhere. Model names are never hardcoded:
//! they are discovered from `/api/tags` and their capabilities from `/api/show`.

use std::{
    io::{BufRead, BufReader},
    process::Command,
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    error::{AppError, AppResult},
    models::AppSettings,
    worker::is_loopback_url,
};

const MAX_JSON_BYTES: u64 = 256 * 1024 * 1024;

pub struct OllamaClient {
    base_url: String,
    agent: ureq::Agent,
}

#[derive(Debug, Clone)]
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub system: &'a str,
    pub user: &'a str,
    /// JSON schema for structured output (Ollama `format`).
    pub format: Option<&'a Value>,
    pub temperature: f64,
    pub num_ctx: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ChatResponse {
    pub content: String,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaModel {
    pub name: String,
    pub size_bytes: i64,
    pub family: Option<String>,
    pub parameter_size: Option<String>,
    pub quantization: Option<String>,
    pub capabilities: Vec<String>,
    /// `chat`, `embedding` or `unknown`.
    pub kind: String,
    pub context_length: Option<i64>,
    /// `comfortable`, `tight` or `too_large` for this machine's RAM.
    pub memory_fit: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaStatus {
    pub base_url: String,
    pub reachable: bool,
    pub version: Option<String>,
    pub models: Vec<OllamaModel>,
    pub system_memory_bytes: Option<u64>,
    pub recommendation: String,
    pub message: Option<String>,
}

impl OllamaClient {
    pub fn new(base_url: &str, timeout: Duration, allow_remote: bool) -> AppResult<Self> {
        let base_url = base_url.trim().trim_end_matches('/').to_owned();
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err(AppError::Ai(
                "La URL de Ollama debe usar http o https".into(),
            ));
        }
        if !allow_remote && !is_loopback_url(&base_url) {
            return Err(AppError::Ai(
                "Ollama debe ejecutarse en este equipo mientras el procesamiento externo esté desactivado".into(),
            ));
        }
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .proxy(None)
            .build();
        Ok(Self {
            base_url,
            agent: config.into(),
        })
    }

    pub fn from_settings(settings: &AppSettings, timeout: Duration) -> AppResult<Self> {
        Self::new(
            &settings.ollama_base_url,
            timeout,
            settings.allow_external_processing && !settings.local_only,
        )
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    fn unreachable(&self, error: ureq::Error) -> AppError {
        AppError::Ai(format!(
            "Ollama no responde en {} ({error}). Abre Ollama o revisa la URL en Configuración.",
            self.base_url
        ))
    }

    fn read_json(&self, response: ureq::http::Response<ureq::Body>) -> AppResult<Value> {
        let status = response.status();
        let mut body = response.into_body();
        let value: Value = body
            .with_config()
            .limit(MAX_JSON_BYTES)
            .read_json()
            .map_err(|error| AppError::Ai(format!("Respuesta inválida de Ollama: {error}")))?;
        if !status.is_success() {
            return Err(ollama_error(status.as_u16(), &value));
        }
        Ok(value)
    }

    pub fn version(&self) -> AppResult<String> {
        let response = self
            .agent
            .get(self.url("/api/version"))
            .call()
            .map_err(|error| self.unreachable(error))?;
        Ok(self.read_json(response)?["version"]
            .as_str()
            .unwrap_or("desconocida")
            .to_owned())
    }

    pub fn list_models(&self) -> AppResult<Vec<OllamaModel>> {
        let response = self
            .agent
            .get(self.url("/api/tags"))
            .call()
            .map_err(|error| self.unreachable(error))?;
        let tags = self.read_json(response)?;
        let mut models = Vec::new();
        for item in tags["models"].as_array().into_iter().flatten() {
            let Some(name) = item["name"].as_str().or_else(|| item["model"].as_str()) else {
                continue;
            };
            let details = self.show(name).unwrap_or(Value::Null);
            let capabilities: Vec<String> = details["capabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect();
            let context_length = details["model_info"].as_object().and_then(|info| {
                info.iter()
                    .find(|(key, _)| key.ends_with(".context_length"))
                    .and_then(|(_, value)| value.as_i64())
            });
            models.push(OllamaModel {
                name: name.to_owned(),
                size_bytes: item["size"].as_i64().unwrap_or(0),
                family: item["details"]["family"].as_str().map(str::to_owned),
                parameter_size: item["details"]["parameter_size"]
                    .as_str()
                    .map(str::to_owned),
                quantization: item["details"]["quantization_level"]
                    .as_str()
                    .map(str::to_owned),
                kind: model_kind(name, &capabilities).into(),
                capabilities,
                context_length,
                memory_fit: None,
            });
        }
        models.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(models)
    }

    pub fn show(&self, model: &str) -> AppResult<Value> {
        let response = self
            .agent
            .post(self.url("/api/show"))
            .send_json(json!({ "model": model }))
            .map_err(|error| self.unreachable(error))?;
        self.read_json(response)
    }

    pub fn embed(&self, model: &str, inputs: &[String]) -> AppResult<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let response = self
            .agent
            .post(self.url("/api/embed"))
            .send_json(json!({ "model": model, "input": inputs, "truncate": true }))
            .map_err(|error| self.unreachable(error))?;
        let value = self.read_json(response)?;
        let embeddings: Vec<Vec<f32>> = value["embeddings"]
            .as_array()
            .ok_or_else(|| AppError::Ai("Ollama no devolvió embeddings".into()))?
            .iter()
            .map(|vector| {
                vector
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_f64)
                            .map(|item| item as f32)
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect();
        if embeddings.len() != inputs.len() || embeddings.iter().any(Vec::is_empty) {
            return Err(AppError::Ai(format!(
                "El modelo {model} no devolvió un embedding por fragmento; verifica que sea un modelo de embeddings"
            )));
        }
        Ok(embeddings)
    }

    /// Streamed chat so `cancelled` is checked between chunks.
    pub fn chat(
        &self,
        request: &ChatRequest<'_>,
        cancelled: &dyn Fn() -> bool,
    ) -> AppResult<ChatResponse> {
        let started = Instant::now();
        let mut body = json!({
            "model": request.model,
            "stream": true,
            "messages": [
                { "role": "system", "content": request.system },
                { "role": "user", "content": request.user },
            ],
            "options": { "temperature": request.temperature, "num_ctx": request.num_ctx },
        });
        if let Some(format) = request.format {
            body["format"] = format.clone();
        }
        let response = self
            .agent
            .post(self.url("/api/chat"))
            .send_json(&body)
            .map_err(|error| self.unreachable(error))?;
        if !response.status().is_success() {
            return Err(self
                .read_json(response)
                .err()
                .unwrap_or_else(|| AppError::Ai("Ollama rechazó la solicitud".into())));
        }
        let mut reader = BufReader::new(response.into_body().into_reader());
        let mut result = ChatResponse::default();
        let mut line = String::new();
        loop {
            if cancelled() {
                return Err(AppError::Cancelled);
            }
            line.clear();
            let read = reader.read_line(&mut line).map_err(|error| {
                AppError::Ai(format!("Se interrumpió la respuesta de Ollama: {error}"))
            })?;
            if read == 0 {
                break;
            }
            if line.trim().is_empty() {
                continue;
            }
            let chunk: Value = serde_json::from_str(line.trim())
                .map_err(|error| AppError::Ai(format!("Fragmento inválido de Ollama: {error}")))?;
            if let Some(error) = chunk["error"].as_str() {
                return Err(AppError::Ai(format!("Ollama: {error}")));
            }
            if let Some(content) = chunk["message"]["content"].as_str() {
                result.content.push_str(content);
            }
            if chunk["done"].as_bool() == Some(true) {
                result.prompt_tokens = chunk["prompt_eval_count"].as_i64();
                result.completion_tokens = chunk["eval_count"].as_i64();
                break;
            }
        }
        result.duration_ms = started.elapsed().as_millis() as i64;
        Ok(result)
    }

    /// Download a model; `on_progress` receives each status line.
    pub fn pull(
        &self,
        model: &str,
        mut on_progress: impl FnMut(&Value) -> AppResult<()>,
        cancelled: &dyn Fn() -> bool,
    ) -> AppResult<()> {
        let response = self
            .agent
            .post(self.url("/api/pull"))
            .send_json(json!({ "model": model, "stream": true }))
            .map_err(|error| self.unreachable(error))?;
        if !response.status().is_success() {
            return Err(self
                .read_json(response)
                .err()
                .unwrap_or_else(|| AppError::Ai("Ollama rechazó la descarga".into())));
        }
        let mut reader = BufReader::new(response.into_body().into_reader());
        let mut line = String::new();
        loop {
            if cancelled() {
                return Err(AppError::Cancelled);
            }
            line.clear();
            if reader
                .read_line(&mut line)
                .map_err(|error| AppError::Ai(format!("Descarga interrumpida: {error}")))?
                == 0
            {
                return Err(AppError::Ai(
                    "La descarga terminó sin confirmación de Ollama".into(),
                ));
            }
            let Ok(progress) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            if let Some(error) = progress["error"].as_str() {
                return Err(AppError::Ai(format!("Ollama: {error}")));
            }
            on_progress(&progress)?;
            if progress["status"].as_str() == Some("success") {
                return Ok(());
            }
        }
    }
}

fn ollama_error(status: u16, body: &Value) -> AppError {
    let message = body["error"].as_str().unwrap_or("error desconocido");
    if status == 404 && message.contains("not found") {
        AppError::Ai(format!("El modelo no está descargado en Ollama: {message}"))
    } else {
        AppError::Ai(format!("Ollama respondió {status}: {message}"))
    }
}

fn model_kind(name: &str, capabilities: &[String]) -> &'static str {
    if capabilities
        .iter()
        .any(|capability| capability == "embedding")
    {
        "embedding"
    } else if capabilities
        .iter()
        .any(|capability| capability == "completion")
    {
        "chat"
    } else if name.to_lowercase().contains("embed") {
        "embedding"
    } else if capabilities.is_empty() {
        "unknown"
    } else {
        "chat"
    }
}

pub fn system_memory_bytes() -> Option<u64> {
    if cfg!(target_os = "macos") {
        let output = Command::new("/usr/sbin/sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        return String::from_utf8_lossy(&output.stdout).trim().parse().ok();
    }
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = meminfo.lines().find(|line| line.starts_with("MemTotal:"))?;
    line.split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()
        .map(|kib| kib * 1024)
}

/// Rough fit of a model's weights (plus runtime overhead) in unified memory.
pub fn memory_fit(size_bytes: i64, memory: u64) -> &'static str {
    let needed = size_bytes.max(0) as f64 * 1.3;
    let ratio = needed / memory as f64;
    if ratio < 0.5 {
        "comfortable"
    } else if ratio < 0.75 {
        "tight"
    } else {
        "too_large"
    }
}

pub fn recommendation(memory: Option<u64>) -> String {
    match memory.map(|bytes| bytes / 1024 / 1024 / 1024) {
        Some(gb) if gb >= 48 => format!(
            "{gb} GB de memoria: caben modelos cuantizados (Q4) de hasta ~30–35 mil millones de parámetros junto con un modelo de embeddings."
        ),
        Some(gb) if gb >= 24 => format!(
            "{gb} GB de memoria: modelos cuantizados de hasta ~14 mil millones de parámetros funcionan con holgura."
        ),
        Some(gb) if gb >= 12 => format!(
            "{gb} GB de memoria: usa modelos de hasta ~8 mil millones de parámetros (Q4) y un modelo de embeddings pequeño; cierra otras apps pesadas."
        ),
        Some(gb) => format!(
            "{gb} GB de memoria: usa modelos de 1–4 mil millones de parámetros; la calidad de las respuestas será limitada."
        ),
        None => {
            "No se pudo leer la memoria del equipo; elige modelos según la RAM disponible.".into()
        }
    }
}

pub fn status(settings: &AppSettings) -> OllamaStatus {
    let memory = system_memory_bytes();
    let mut status = OllamaStatus {
        base_url: settings.ollama_base_url.clone(),
        reachable: false,
        version: None,
        models: Vec::new(),
        system_memory_bytes: memory,
        recommendation: recommendation(memory),
        message: None,
    };
    let client = match OllamaClient::from_settings(settings, Duration::from_secs(10)) {
        Ok(client) => client,
        Err(error) => {
            status.message = Some(error.to_string());
            return status;
        }
    };
    match client.version() {
        Ok(version) => {
            status.reachable = true;
            status.version = Some(version);
        }
        Err(_) => {
            status.message = Some(format!(
                "Ollama no está disponible en {}. Instálalo desde ollama.com y ábrelo; la biblioteca, la extracción y la búsqueda léxica siguen funcionando sin IA.",
                settings.ollama_base_url
            ));
            return status;
        }
    }
    match client.list_models() {
        Ok(mut models) => {
            if let Some(memory) = memory {
                for model in &mut models {
                    model.memory_fit = Some(memory_fit(model.size_bytes, memory).into());
                }
            }
            if models.is_empty() {
                status.message =
                    Some("Ollama está activo pero no tiene modelos descargados.".into());
            }
            status.models = models;
        }
        Err(error) => status.message = Some(error.to_string()),
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_remote_ollama_without_authorization() {
        assert!(
            OllamaClient::new("https://ollama.example.org", Duration::from_secs(1), false).is_err()
        );
        assert!(
            OllamaClient::new("http://127.0.0.1:11434/", Duration::from_secs(1), false).is_ok()
        );
        assert!(
            OllamaClient::new("https://ollama.example.org", Duration::from_secs(1), true).is_ok()
        );
    }

    #[test]
    fn classifies_models_and_memory_fit() {
        assert_eq!(model_kind("nomic-embed-text:latest", &[]), "embedding");
        assert_eq!(
            model_kind("any", &["completion".into(), "tools".into()]),
            "chat"
        );
        assert_eq!(model_kind("any", &["embedding".into()]), "embedding");
        let gib = 1024_u64.pow(3);
        assert_eq!(memory_fit(5 * gib as i64, 64 * gib), "comfortable");
        assert_eq!(memory_fit(9 * gib as i64, 16 * gib), "tight");
        assert_eq!(memory_fit(20 * gib as i64, 16 * gib), "too_large");
        assert!(recommendation(Some(16 * gib)).contains("8 mil millones"));
    }
}
